// SPDX-License-Identifier: GPL-3.0-only
mod stream_tests;

use crate::Error;
use crate::native_window::NativeWindow;
use crate::platform::{Event, NativeSurface, SIDE, SURFACE_SIZE, TestError};
use crate::renderer::AndroidRenderer;
use lavashell_core::prelude::{
    Density, DpExt, FrameTime, OutputProfile, OutputRequest, Precision, SceneSnapshot, SceneTree,
    Size, Viewport,
};
use lavashell_renderer::{
    Error as RendererError, Fonts, FrameOutcome, PresentationPolicy, SurfaceAlpha, SurfaceTiming,
};
use log::SetLoggerError;
use std::{cell::RefCell, sync::OnceLock, time::Duration};

thread_local! {
    static WARNINGS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

struct Recorder;

impl log::Log for Recorder {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::Level::Warn
    }

    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            WARNINGS.with_borrow_mut(|warnings| warnings.push(record.args().to_string()));
        }
    }

    fn flush(&self) {}
}

fn warnings(run: impl FnOnce() -> Result<(), TestError>) -> Result<Vec<String>, TestError> {
    static INSTALL: OnceLock<Result<(), SetLoggerError>> = OnceLock::new();
    let installed = INSTALL.get_or_init(|| {
        log::set_logger(&Recorder)?;
        log::set_max_level(log::LevelFilter::Warn);
        Ok(())
    });
    if let Err(error) = installed {
        return Err(TestError::InstallLogger(error));
    }
    WARNINGS.with_borrow_mut(Vec::clear);
    run()?;
    Ok(WARNINGS.take())
}

fn window(surface: &NativeSurface, timing: SurfaceTiming) -> NativeWindow {
    NativeWindow::new(surface.window(), timing)
}

/// The whole simulated surface at one pixel per dp.
fn viewport() -> Viewport {
    Viewport::new(Size::new(SIDE.dp(), SIDE.dp()), Density::ONE)
}

fn empty() -> SceneSnapshot {
    SceneTree::default().snapshot()
}

fn render(renderer: &mut AndroidRenderer) -> Result<FrameOutcome, Error> {
    renderer.render(&empty(), FrameTime::default(), viewport())
}

fn render_at(
    renderer: &mut AndroidRenderer,
    presentation_time_ns: u64,
) -> Result<FrameOutcome, Error> {
    renderer.render_at(
        &empty(),
        FrameTime::default(),
        viewport(),
        presentation_time_ns,
    )
}

fn assert_untimed_rejected(renderer: &mut AndroidRenderer, surface: &NativeSurface) {
    surface.clear_events();
    assert!(matches!(
        render(renderer),
        Err(Error::Draw(RendererError::ExplicitTimestampRequired))
    ));
    assert_eq!(surface.acquisitions(), 0);
}

#[test]
fn reattaching_a_retained_window_rejects_untimed_draws_before_acquiring() -> Result<(), TestError> {
    let surface = NativeSurface::new();
    let timing = SurfaceTiming::new(PresentationPolicy::Automatic);
    let mut renderer = AndroidRenderer::new(&Fonts::new())?;
    renderer.attach(window(&surface, timing), SurfaceAlpha::Opaque)?;
    // The three references to the surface belong to the fixture, the window and the target of the attachment.
    assert_eq!(surface.references(), 3);
    assert_eq!(render_at(&mut renderer, 5_000)?, FrameOutcome::Shown);
    assert_eq!(
        surface.presentations(),
        vec![Some(Duration::from_nanos(5_000))]
    );

    let retained = renderer.detach().ok_or(TestError::NothingAttached)?;
    assert_eq!(surface.references(), 2);
    assert_eq!(surface.last_event(), Some(Event::NativeReleased));
    assert_eq!(render(&mut renderer)?, FrameOutcome::Pending);
    renderer.attach(retained, SurfaceAlpha::Opaque)?;
    assert_untimed_rejected(&mut renderer, &surface);
    assert_eq!(render_at(&mut renderer, 19_000)?, FrameOutcome::Shown);
    assert_eq!(
        surface.presentations(),
        vec![Some(Duration::from_micros(19))]
    );

    surface.clear_events();
    assert!(renderer.detach().is_some());
    assert_eq!(
        surface.events(),
        vec![
            Event::TargetDropped,
            Event::NativeReleased,
            Event::NativeReleased
        ]
    );
    assert_eq!(surface.references(), 1);

    let fresh = NativeSurface::new();
    renderer.attach(
        window(&fresh, SurfaceTiming::new(PresentationPolicy::Automatic)),
        SurfaceAlpha::Opaque,
    )?;
    assert_eq!(render(&mut renderer)?, FrameOutcome::Shown);
    assert_eq!(render(&mut renderer)?, FrameOutcome::Shown);
    assert_eq!(fresh.acquisitions(), 2);
    assert_eq!(fresh.presentations(), vec![None, None]);
    assert_eq!(
        renderer.size()?,
        Some((SURFACE_SIZE.width, SURFACE_SIZE.height))
    );
    assert_eq!(renderer.device_name(), "simulated Vulkan boundary");
    assert!(renderer.supports_presentation_time());
    assert_eq!(renderer.output().map(|output| output.fallback), Some(false));
    let deeper = OutputRequest::new(OutputProfile::DisplayP3, Precision::Ten);
    renderer.request_output(deeper)?;
    assert_eq!(render(&mut renderer)?, FrameOutcome::Shown);
    assert_eq!(
        renderer
            .output()
            .map(|output| (output.requested, output.fallback)),
        Some((deeper, true))
    );
    drop(renderer);
    assert_eq!(fresh.references(), 1);
    Ok(())
}

#[test]
fn reacquisition_keeps_declared_and_unknown_explicit_policies() -> Result<(), TestError> {
    let surface = NativeSurface::new();
    let declared = SurfaceTiming::new(PresentationPolicy::Explicit);
    let mut renderer = AndroidRenderer::new(&Fonts::new())?;
    renderer.attach(window(&surface, declared.clone()), SurfaceAlpha::Opaque)?;
    assert_untimed_rejected(&mut renderer, &surface);
    assert!(renderer.detach().is_some());
    assert_eq!(surface.references(), 1);

    renderer.attach(window(&surface, declared), SurfaceAlpha::Opaque)?;
    assert_untimed_rejected(&mut renderer, &surface);
    assert!(renderer.detach().is_some());

    // The timing history of a new Surface is unknown, so the window is declared Explicit when it is acquired.
    let unknown = NativeSurface::new();
    renderer.attach(
        window(&unknown, SurfaceTiming::new(PresentationPolicy::Explicit)),
        SurfaceAlpha::Opaque,
    )?;
    assert_untimed_rejected(&mut renderer, &unknown);
    drop(renderer);
    assert_eq!(surface.references(), 1);
    assert_eq!(unknown.references(), 1);
    Ok(())
}

#[test]
fn pending_acquisition_does_not_latch_policy_or_reuse_its_timestamp() -> Result<(), TestError> {
    let surface = NativeSurface::new();
    let timing = SurfaceTiming::new(PresentationPolicy::Automatic);
    let mut renderer = AndroidRenderer::new(&Fonts::new())?;
    renderer.attach(window(&surface, timing), SurfaceAlpha::Opaque)?;
    surface.make_next_acquisition_pending();
    assert_eq!(render_at(&mut renderer, 5_000)?, FrameOutcome::Pending);
    assert_eq!(surface.presentations(), vec![]);
    assert_eq!(render(&mut renderer)?, FrameOutcome::Shown);
    assert_eq!(surface.presentations(), vec![None]);
    assert_eq!(render_at(&mut renderer, 19_000)?, FrameOutcome::Shown);
    assert_eq!(
        surface.presentations(),
        vec![None, Some(Duration::from_micros(19))]
    );
    assert_untimed_rejected(&mut renderer, &surface);
    Ok(())
}
