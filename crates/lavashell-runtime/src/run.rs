// SPDX-License-Identifier: GPL-3.0-only
//! Runs an app on a platform: the views on the calling thread, the platform on a thread of its own, and the actors
//! between them on theirs.

use crate::deadline_receiver::DeadlineReceiver;
use crate::view_actor::{StopNotice, ViewActor, ViewInput};
use crate::{
    ActionQueue, ClipboardTransfers, Drags, Error, InputFilter, LayerSurface, LockConfiguration,
    Observers, PanelsPerOutput, RendererError, RunEnd, Stopped, ViewError, Watches, Window,
};
use lavashell_actor::{JoinError, SpawnError, mailbox};
use lavashell_core::preferences::PreferenceOverrides;
use lavashell_core::view::View;
use lavashell_platform::{Platform, PlatformLinks, Reports};
use lavashell_renderer::{RendererSetup, SurfaceControl};
use std::sync::Arc;
use std::time::{Duration, Instant};
use std::{io, thread};

/// A surface that the app opens when it starts.
pub enum SurfaceSetup {
    Window(Window, View),
    Panel(LayerSurface, View),
}

/// The configuration of a run: the surfaces to open, the renderer setup, and the callbacks and queues through which
/// the app follows the run.
pub struct RuntimeSetup {
    pub surfaces: Vec<SurfaceSetup>,
    pub panels_per_output: Option<PanelsPerOutput<LayerSurface>>,
    pub session_lock: Option<LockConfiguration>,
    pub renderer: RendererSetup,
    /// The start of the run, from which every time of the run is measured.
    pub start: Instant,
    /// The GPU time per frame available to the effects of each surface, or `None` for no limit.
    pub ui_budget: Option<Duration>,
    pub run_end: RunEnd,
    pub preference_overrides: PreferenceOverrides,
    pub input_filter: Option<InputFilter>,
    pub observers: Observers,
    pub watches: Watches,
    pub clipboard: ClipboardTransfers,
    pub action_queue: ActionQueue,
    pub drags: Drags,
}

/// A failure of a run, named after the part that failed. `E` is the error of the platform, and `C` the error of the
/// controls of the native surfaces.
#[derive(Debug, thiserror::Error)]
pub enum RunError<E, C> {
    #[error(transparent)]
    Platform(E),
    #[error("the thread of the platform could not be started")]
    SpawnPlatform(#[source] io::Error),
    #[error("the thread of the platform panicked")]
    PlatformPanicked,
    #[error("the renderer could not start")]
    StartRenderer(#[source] SpawnError<RendererError<C>>),
    #[error("the renderer failed")]
    RunRenderer(#[source] JoinError<RendererError<C>>),
    #[error("the shaper that measures text for the views could not be created")]
    CreateShaper(#[source] lavashell_renderer::Error),
    #[error("the choreographer or the scene builder failed")]
    SceneFailed(#[source] Arc<Error>),
    #[error(transparent)]
    View(#[from] ViewError),
}

/// Connects to the host of `P` and runs the app of `setup` until it ends. The views run on the calling thread and
/// the platform on a thread of its own.
///
/// # Errors
///
/// Fails when the platform, a view or an actor fails. When a part of the run stopped and the views end because of
/// it, the failure of that part replaces the stop. Otherwise the run fails with the first failure among the renderer,
/// the frame scheduler, the scene pipeline and the platform, in that order.
pub fn drive<P: Platform>(
    setup: RuntimeSetup,
) -> Result<(), RunError<P::Error, <P::Control as SurfaceControl>::Error>> {
    let start = setup.start;
    let connection = P::connect().map_err(RunError::Platform)?;
    let (inbox, inputs) = mailbox();
    let inputs = DeadlineReceiver::new(inputs).map_err(ViewError::Runtime)?;
    let (requests, platform_requests) = mailbox();
    let mut view = ViewActor::<P::Control>::start(
        setup,
        connection.presentation,
        connection.host,
        requests,
        &inbox,
    )?;
    let (scheduler, renderer) = view.links().map_err(ViewError::from)?;
    let links = PlatformLinks {
        reports: Reports {
            view: inbox.outlet(ViewInput::Platform),
            scheduler,
            renderer,
            start,
        },
        requests: platform_requests,
    };
    let notice = StopNotice::new(&inbox, Stopped::Platform);
    let platform = connection.platform;
    let served = thread::Builder::new()
        .name("platform".to_owned())
        .spawn(move || {
            let _notice_on_end = notice;
            platform.serve(links)
        })
        .map_err(RunError::SpawnPlatform)?;
    drop(inbox);
    let ran = view.run(&inputs);
    let (ran, stops) = view.stop(&inputs, ran);
    let renderer = stops.renderer.map_err(RunError::RunRenderer);
    let scheduler = stops
        .scheduler
        .map_err(|error| RunError::View(ViewError::Runtime(error)));
    let scene = stops.scene.map_err(RunError::SceneFailed);
    let platform = match served.join() {
        Ok(served) => served.map_err(RunError::Platform),
        Err(_panic) => Err(RunError::PlatformPanicked),
    };
    let Err(error) = ran else {
        return renderer.and(scheduler).and(scene).and(platform);
    };
    let stopped = match error {
        ViewError::Stopped(Stopped::Renderer) => renderer,
        ViewError::Stopped(Stopped::Scene) => scene,
        ViewError::Stopped(Stopped::Scheduler) => scheduler,
        ViewError::Stopped(Stopped::Platform) => platform,
        _ => Ok(()),
    };
    stopped.and(Err(RunError::View(error)))
}

#[cfg(test)]
mod tests;
