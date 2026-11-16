// SPDX-License-Identifier: GPL-3.0-only
mod replies;
mod sockets;

use crate::CompositorRenderer;
use crate::error::HarnessError;
use lavashell_process_scope::{ScopedChild, first_ready};
use replies::{CommandResult, Node, Output};
pub use replies::{OutputSize, SwayWindow};
use serde::de::DeserializeOwned;
use sockets::SwaySockets;
use std::ffi::OsString;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::os::fd::AsFd;
use std::path::Path;
use std::process::{Command, Stdio};
use tempfile::TempDir;

const CONFIG: &str = "default_border none\ndefault_floating_border none\nxwayland disable\nseat seat0 fallback true\n";
const HIDDEN_ENVIRONMENT: [&str; 3] = ["DISPLAY", "WAYLAND_DISPLAY", "SWAYSOCK"];
const WINDOW_EVENTS: [&str; 2] = ["tick", "window"];

pub struct HeadlessSway {
    compositor: ScopedChild,
    environment: Vec<(OsString, OsString)>,
    display: OsString,
    runtime: TempDir,
}

#[derive(Clone, Copy)]
enum WindowWake {
    Event,
    CompositorExited,
    ClientExited,
}

impl HeadlessSway {
    pub fn start(
        drm_device: Option<&Path>,
        renderer: CompositorRenderer,
        log: File,
    ) -> Result<Self, HarnessError> {
        let runtime = (tempfile::Builder::new().prefix("headless-sway-").tempdir())
            .map_err(HarnessError::CreateRuntime)?;
        let config = runtime.path().join("sway.conf");
        std::fs::write(&config, CONFIG).map_err(HarnessError::WriteConfig)?;
        let mut environment: Vec<(OsString, OsString)> = std::env::vars_os()
            .filter(|(name, _)| !HIDDEN_ENVIRONMENT.iter().any(|hidden| name == hidden))
            .collect();
        environment.extend(
            [
                ("XDG_RUNTIME_DIR", runtime.path().as_os_str()),
                ("WLR_BACKENDS", "headless".as_ref()),
                ("WLR_LIBINPUT_NO_DEVICES", "1".as_ref()),
                ("WLR_RENDERER", renderer.wlroots_name().as_ref()),
            ]
            .into_iter()
            .chain(drm_device.map(|device| ("WLR_RENDER_DRM_DEVICE", device.as_os_str())))
            .map(|(name, value)| (OsString::from(name), value.to_owned())),
        );
        let watch = SwaySockets::watch(runtime.path())?;
        let errors = log.try_clone().map_err(HarnessError::DuplicateLog)?;
        let compositor = ScopedChild::spawn(
            Command::new("sway")
                .arg("--unsupported-gpu")
                .arg("--config")
                .arg(&config)
                .env_clear()
                .envs(environment.iter().map(|(name, value)| (name, value)))
                .stdout(log)
                .stderr(errors),
        )?;
        let sockets = watch.wait(&compositor)?;
        environment.extend([
            (OsString::from("SWAYSOCK"), sockets.ipc.into_os_string()),
            (OsString::from("WAYLAND_DISPLAY"), sockets.display.clone()),
        ]);
        Ok(Self {
            compositor,
            environment,
            display: sockets.display,
            runtime,
        })
    }

    pub fn runtime(&self) -> &Path {
        self.runtime.path()
    }

    pub fn display(&self) -> &OsString {
        &self.display
    }

    pub fn environment(&self) -> impl Iterator<Item = (&OsString, &OsString)> {
        self.environment.iter().map(|(name, value)| (name, value))
    }

    fn swaymsg(&self) -> Command {
        let mut command = Command::new("swaymsg");
        let _chained_command = command.env_clear().envs(self.environment());
        command
    }

    fn reply<Model: DeserializeOwned>(&self, request: &[&str]) -> Result<Model, HarnessError> {
        let request_words = || request.iter().map(|&word| word.to_owned()).collect();
        let answer = (self.swaymsg().args(request).output()).map_err(HarnessError::StartSwaymsg)?;
        if answer.stdout.is_empty() {
            return Err(HarnessError::SwayRequestFailed {
                request: request_words(),
                status: answer.status,
                stderr: String::from_utf8_lossy(&answer.stderr).into_owned(),
            });
        }
        serde_json::from_slice(&answer.stdout).map_err(|source| HarnessError::UnreadableSwayReply {
            request: request_words(),
            source,
        })
    }

    pub fn command(&self, arguments: &[&str]) -> Result<(), HarnessError> {
        let results: Vec<CommandResult> = self.reply(arguments)?;
        if results.iter().all(|result| result.success) {
            Ok(())
        } else {
            Err(HarnessError::SwayCommandFailed {
                command: arguments.iter().map(|&word| word.to_owned()).collect(),
                errors: results
                    .into_iter()
                    .filter_map(|result| result.error)
                    .collect(),
            })
        }
    }

    pub fn output_size(&self) -> Result<OutputSize, HarnessError> {
        match self
            .reply::<Vec<Output>>(&["-t", "get_outputs"])?
            .as_slice()
        {
            [output] => Ok(output.rect),
            outputs => Err(HarnessError::UnexpectedOutputs(outputs.len())),
        }
    }

    pub fn windows(&self) -> Result<Vec<SwayWindow>, HarnessError> {
        Ok(self.reply::<Node>(&["-t", "get_tree"])?.windows())
    }

    pub fn wait_for_windows(
        &self,
        ready: impl Fn(&[SwayWindow]) -> bool,
        client: Option<&ScopedChild>,
    ) -> Result<Vec<SwayWindow>, HarnessError> {
        let subscription =
            serde_json::to_string(&WINDOW_EVENTS).map_err(HarnessError::EncodeSubscription)?;
        let mut events = ScopedChild::spawn(
            self.swaymsg()
                .args(["-t", "subscribe", "-m", &subscription])
                .stdout(Stdio::piped()),
        )?;
        let mut event_lines =
            BufReader::new(events.take_stdout().ok_or(HarnessError::NoEventPipe)?);
        let event_readiness = (event_lines.get_ref().as_fd().try_clone_to_owned())
            .map_err(HarnessError::ReadWindowEvents)?;
        let sources: Vec<_> = [
            (event_readiness.as_fd(), WindowWake::Event),
            (self.compositor.exit_event(), WindowWake::CompositorExited),
        ]
        .into_iter()
        .chain(client.map(|client| (client.exit_event(), WindowWake::ClientExited)))
        .collect();
        loop {
            if event_lines.buffer().is_empty() {
                match first_ready(&sources)? {
                    WindowWake::ClientExited => return Err(HarnessError::AppEndedBeforeItsWindow),
                    WindowWake::CompositorExited => return Err(HarnessError::CompositorExited),
                    WindowWake::Event => {}
                }
            }
            let received = event_lines
                .fill_buf()
                .map_err(HarnessError::ReadWindowEvents)?
                .len();
            if received == 0 {
                return Err(HarnessError::WindowEventsEnded);
            }
            event_lines.consume(received);
            let windows = self.windows()?;
            if ready(&windows) {
                return Ok(windows);
            }
        }
    }
}

impl CompositorRenderer {
    fn wlroots_name(self) -> &'static str {
        match self {
            Self::Gles2 => "gles2",
            Self::Pixman => "pixman",
        }
    }
}
