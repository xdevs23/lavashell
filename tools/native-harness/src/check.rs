// SPDX-License-Identifier: GPL-3.0-only
use crate::CheckOptions;
use crate::capture::OutputCapture;
use crate::error::HarnessError;
use crate::launch::{Run, run_app};
use crate::nix_apps::Flake;
use crate::report::Report;
use crate::run_files::RunFiles;
use crate::sway::{HeadlessSway, SwayWindow};

pub fn check_every_app(options: &CheckOptions) -> Result<(), HarnessError> {
    let flake = match &options.flake {
        Some(reference) => Flake::named(reference.clone())?,
        None => Flake::of_repository()?,
    };
    let directory = match &options.output_dir {
        Some(directory) => directory.clone(),
        None => (tempfile::Builder::new().prefix("native-launch-").tempdir())
            .map_err(HarnessError::CreateTemporaryDirectory)?
            .keep(),
    };
    let files = RunFiles::in_directory(directory, options.label.clone())?;
    let apps = flake.wayland_apps(&options.only)?;
    let home = (tempfile::Builder::new()
        .prefix("native-launch-home-")
        .tempdir())
    .map_err(HarnessError::CreateTemporaryDirectory)?;
    let sway = HeadlessSway::start(
        options.drm_device.as_deref(),
        options.renderer,
        files.create("compositor.log")?,
    )?;
    let mut capture = OutputCapture::start(&sway)?;
    let empty_screenshot = files.path("empty.png");
    capture.capture_change(&empty_screenshot, None)?;
    let empty_output =
        std::fs::read(&empty_screenshot).map_err(|source| HarnessError::ReadImage {
            path: empty_screenshot,
            source,
        })?;
    let mut run = Run {
        flake: &flake,
        files: &files,
        home: home.path(),
        sway: &sway,
        capture: &mut capture,
        empty_output: &empty_output,
    };
    let mut examples = Vec::with_capacity(apps.len());
    for app in &apps {
        let _empty_window_list = sway.wait_for_windows(<[SwayWindow]>::is_empty, None)?;
        examples.push(run_app(app, &mut run)?);
        println!("Passed nix run #{} with empty PATH", app.name);
    }
    let report = Report {
        flake: flake.to_string(),
        all_wayland_apps: options.only.is_empty(),
        examples,
    };
    let report_path = files.report_path();
    report.write(&report_path)?;
    println!(
        "Verified {} nix run apps; report {}",
        apps.len(),
        report_path.display()
    );
    Ok(())
}
