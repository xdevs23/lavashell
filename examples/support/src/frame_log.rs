// SPDX-License-Identifier: GPL-3.0-only
use crate::Error;
use lavashell::{core::frame::PresentationFlag, prelude::*, wayland};
use serde::Serialize;
use std::{cell::RefCell, ffi::OsStr, rc::Rc};

/// Runs `app` and, when `LAVASHELL_FRAME_LOG` is set to a file path, writes its frame reports there as CSV.
///
/// # Errors
/// Returns an error when the app fails or the frame log cannot be written.
pub fn run(
    app: App,
    startup: impl FnOnce(&mut App) -> Result<(), wayland::Error>,
) -> Result<(), Error> {
    logged(app, None, startup)
}

/// Like [`run`], and passes every frame report to `observer` as well.
///
/// # Errors
/// Returns an error when the app fails or the frame log cannot be written.
pub fn run_observed(
    app: App,
    observer: impl FnMut(FrameReport) + 'static,
    startup: impl FnOnce(&mut App) -> Result<(), wayland::Error>,
) -> Result<(), Error> {
    logged(app, Some(Box::new(observer)), startup)
}

fn logged(
    app: App,
    mut observer: Option<Box<dyn FnMut(FrameReport)>>,
    startup: impl FnOnce(&mut App) -> Result<(), wayland::Error>,
) -> Result<(), Error> {
    let path = std::env::var_os("LAVASHELL_FRAME_LOG");
    let reports = Rc::new(RefCell::new(Vec::new()));
    let log = path.is_some().then(|| reports.clone());
    let app = if log.is_some() {
        app.gpu_timing(true)
    } else {
        app
    };
    let app = if observer.is_some() || log.is_some() {
        app.frame_observer(move |report| {
            if let Some(observer) = &mut observer {
                observer(report);
            }
            if let Some(log) = &log {
                log.borrow_mut().push(report);
            }
        })
    } else {
        app
    };

    app.run(startup).map_err(Error::App)?;

    match path {
        Some(path) => write_frame_reports(&path, &reports.borrow()),
        None => Ok(()),
    }
}

/// Called after the app exits, so writing the file does not delay any frame of the render loop.
fn write_frame_reports(path: &OsStr, reports: &[FrameReport]) -> Result<(), Error> {
    let mut file = csv::Writer::from_path(path).map_err(Error::CreateFrameLog)?;
    for report in reports {
        file.serialize(FrameRow::from(report))
            .map_err(Error::WriteFrameReport)?;
    }
    file.flush().map_err(Error::FlushFrameLog)
}

#[derive(Serialize)]
struct FrameRow {
    surface: u64,
    sequence: u64,
    stage: FrameStage,
    now_ns: u128,
    sample_ns: u128,
    observed_ns: u128,
    refresh_ns: Option<u128>,
    mapped_timestamp: u8,
    vsync: Option<u8>,
    hardware_clock: Option<u8>,
    hardware_completion: Option<u8>,
    zero_copy: Option<u8>,
    animation_delta_ns: Option<u128>,
    display_delta_ns: Option<u128>,
    animation_error_ns: Option<i128>,
    render_ns: Option<u128>,
    cpu_ns: Option<u128>,
    root_compositions: Option<u64>,
    component_compositions: Option<u64>,
    painted_components: Option<u64>,
    reused_components: Option<u64>,
    measured_components: Option<u64>,
    reused_layouts: Option<u64>,
    cache_hits: Option<u64>,
    cache_misses: Option<u64>,
    uncached_leaves: Option<u64>,
    skipped_leaves: Option<u64>,
    cache_bytes: Option<usize>,
    damaged_pixels: Option<u64>,
    scene_generation: Option<u64>,
    gpu_sequence: Option<u64>,
    gpu_ns: Option<u128>,
}

impl From<&FrameReport> for FrameRow {
    fn from(report: &FrameReport) -> Self {
        let has = |flag| report.flags.map(|flags| u8::from(flags.contains(flag)));
        let metrics = report.metrics;
        let submission = report.submission;
        let view = submission.map(|s| s.view);
        let render = submission.map(|s| s.render);
        let gpu = submission.and_then(|s| s.gpu);
        Self {
            surface: report.surface.0,
            sequence: report.timing.sequence,
            stage: report.stage,
            now_ns: report.timing.now.as_nanos(),
            sample_ns: report.timing.presentation.as_nanos(),
            observed_ns: report.observed.as_nanos(),
            refresh_ns: report.timing.refresh.map(|r| r.as_nanos()),
            mapped_timestamp: u8::from(report.presentation_time.is_some()),
            vsync: has(PresentationFlag::Vsync),
            hardware_clock: has(PresentationFlag::HardwareClock),
            hardware_completion: has(PresentationFlag::HardwareCompletion),
            zero_copy: has(PresentationFlag::ZeroCopy),
            animation_delta_ns: metrics.map(|m| m.animation_step.as_nanos()),
            display_delta_ns: metrics.map(|m| m.display_step.as_nanos()),
            animation_error_ns: metrics.map(|m| m.animation_error_ns),
            render_ns: submission.map(|s| s.render_cost.as_nanos()),
            cpu_ns: submission.map(|s| s.cpu_time.as_nanos()),
            root_compositions: view.map(|v| v.root_compositions),
            component_compositions: view.map(|v| v.component_compositions),
            painted_components: view.map(|v| v.painted_components),
            reused_components: view.map(|v| v.reused_components),
            measured_components: view.map(|v| v.measured_components),
            reused_layouts: view.map(|v| v.reused_layouts),
            cache_hits: render.map(|r| r.cache_hits),
            cache_misses: render.map(|r| r.cache_misses),
            uncached_leaves: render.map(|r| r.uncached_leaves),
            skipped_leaves: render.map(|r| r.skipped_leaves),
            cache_bytes: render.map(|r| r.cache_bytes),
            damaged_pixels: render.map(|r| r.damaged_pixels),
            scene_generation: submission.and_then(|s| s.scene),
            gpu_sequence: gpu.map(|gpu| gpu.sequence),
            gpu_ns: gpu.map(|gpu| gpu.elapsed.as_nanos()),
        }
    }
}
