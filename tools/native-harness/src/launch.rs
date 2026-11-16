// SPDX-License-Identifier: GPL-3.0-only
use crate::app_declarations::{LaunchKind, WaylandApp};
use crate::capture::OutputCapture;
use crate::child_lines::ChildLines;
use crate::error::HarnessError;
use crate::helper;
use crate::nix_apps::Flake;
use crate::process_facts::{environment_value_of, executable_of};
use crate::report::AppResult;
use crate::run_files::RunFiles;
use crate::sway::HeadlessSway;
use lavashell_process_scope::ScopedChild;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Instant;

const EMPTY_PATH: &str = "";
const LANGUAGE: &str = "C.UTF-8";
const EVERY_WINDOW: [&str; 2] = ["[app_id=\".*\"]", "kill"];

pub struct Run<'a> {
    pub flake: &'a Flake,
    pub files: &'a RunFiles,
    pub home: &'a Path,
    pub sway: &'a HeadlessSway,
    pub capture: &'a mut OutputCapture,
    pub empty_output: &'a [u8],
}

pub fn run_app(app: &WaylandApp, run: &mut Run<'_>) -> Result<AppResult, HarnessError> {
    let executable = wrapped_executable(app)?;
    let app_home = run.home.join(&app.name);
    std::fs::create_dir(&app_home).map_err(|source| HarnessError::CreateAppHome {
        app: app.name.clone(),
        source,
    })?;
    let started = Instant::now();
    let log = run.files.create(&format!("{}.log", app.name))?;
    let errors = log.try_clone().map_err(HarnessError::DuplicateLog)?;
    let mut command = run.flake.run_command(app);
    let _chained_command = command
        .current_dir(&app_home)
        .env_clear()
        .envs([
            ("PATH", OsStr::new(EMPTY_PATH)),
            ("HOME", app_home.as_os_str()),
            ("LANG", OsStr::new(LANGUAGE)),
            ("XDG_RUNTIME_DIR", run.sway.runtime().as_os_str()),
            ("WAYLAND_DISPLAY", run.sway.display()),
        ])
        .stdin(Stdio::piped())
        .stderr(errors);
    let screenshot = match app.launch.kind {
        LaunchKind::Window => {
            let client = ScopedChild::spawn(command.stdout(log))?;
            let pid = match run
                .sway
                .wait_for_windows(|windows| !windows.is_empty(), Some(&client))?
                .as_slice()
            {
                [window] => window.pid,
                windows => return Err(HarnessError::UnexpectedWindows(windows.to_vec())),
            };
            check_launched_process(app, pid, &executable)?;
            let screenshot = run.files.path(&format!("{}.png", app.name));
            run.capture
                .capture_change_from(&screenshot, run.empty_output, &client)?;
            run.sway.command(&EVERY_WINDOW)?;
            check_exit(app, &client)?;
            Some(screenshot)
        }
        LaunchKind::Helper => {
            let mut client = ScopedChild::spawn(command.stdout(Stdio::piped()))?;
            let output = client.take_stdout().ok_or(HarnessError::NoHelperPipes)?;
            let mut replies = ChildLines::of(output).map_err(HarnessError::ReadHelperReply)?;
            let pid = helper::await_ready(&mut replies, &client)?;
            check_launched_process(app, pid, &executable)?;
            helper::press_space_at(run.sway.output_size()?.center(), &mut client, &mut replies)?;
            check_exit(app, &client)?;
            None
        }
    };
    Ok(AppResult {
        name: app.name.clone(),
        seconds: started.elapsed().as_secs_f64(),
        screenshot: screenshot.and_then(|path| path.file_name().map(PathBuf::from)),
        executable,
    })
}

// https://github.com/NixOS/nixpkgs/blob/master/pkgs/build-support/setup-hooks/make-wrapper.sh
fn wrapped_executable(app: &WaylandApp) -> Result<PathBuf, HarnessError> {
    let not_built = || HarnessError::PackageNotBuilt {
        app: app.name.clone(),
    };
    if !app.program.is_file() {
        return Err(not_built());
    }
    let name = app.program.file_name().ok_or_else(not_built)?;
    let mut wrapped = OsString::from(".");
    wrapped.push(name);
    wrapped.push("-wrapped");
    app.program
        .with_file_name(wrapped)
        .canonicalize()
        .map_err(|source| HarnessError::FindWrappedExecutable {
            app: app.name.clone(),
            source,
        })
}

fn check_launched_process(app: &WaylandApp, pid: u32, expected: &Path) -> Result<(), HarnessError> {
    let executable = executable_of(pid).map_err(|source| HarnessError::ReadExecutable {
        app: app.name.clone(),
        source,
    })?;
    if executable != expected {
        return Err(HarnessError::UnexpectedExecutable {
            app: app.name.clone(),
            executable,
        });
    }
    let path =
        environment_value_of(pid, "PATH").map_err(|source| HarnessError::ReadEnvironment {
            app: app.name.clone(),
            source,
        })?;
    if path.as_deref() == Some(OsStr::new(EMPTY_PATH)) {
        Ok(())
    } else {
        Err(HarnessError::PathNotEmpty {
            app: app.name.clone(),
            path,
        })
    }
}

fn check_exit(app: &WaylandApp, client: &ScopedChild) -> Result<(), HarnessError> {
    let exit = client.wait()?;
    if exit.success() {
        Ok(())
    } else {
        Err(HarnessError::AppFailed {
            app: app.name.clone(),
            exit,
        })
    }
}
