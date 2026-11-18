// SPDX-License-Identifier: GPL-3.0-only

use crate::app::App;
use crate::clipboard::ClipboardTransfers;
use crate::deadline_receiver::DeadlineReceiver;
use crate::drag::Drags;
use crate::input::InputFilter;
use crate::layer_surface::LayerSurface;
use crate::observers::Observers;
use crate::panels::PanelsPerOutput;
use crate::rendered_platform::RenderedPlatform;
use crate::renderer_actor::PlatformRendererError;
use crate::run_end::RunEnd;
use crate::session_lock::LockConfiguration;
use crate::view_actions::ActionQueue;
use crate::view_actor::{Connected, StopNotice, Stopped, ViewActor, ViewError, ViewInput};
use crate::watches::Watches;
use crate::window::Window;
use crate::{logging, registry, scene_pipeline};
use lavashell_actor::{JoinError, SpawnError, mailbox, mailbox_on};
use lavashell_core::preferences::PreferenceOverrides;
use lavashell_core::view::View;
use lavashell_platform::{Capability, PlatformLinks};
use lavashell_vulkan_renderer::RendererSetup;
use std::collections::BTreeSet;
use std::fmt;
use std::sync::Arc;
use std::{io, thread};

pub(crate) enum SurfaceSetup {
    Window(Window, View),
    Panel(LayerSurface, View),
}

pub(crate) struct RuntimeSetup {
    pub surfaces: Vec<SurfaceSetup>,
    pub panels_per_output: Option<PanelsPerOutput<LayerSurface>>,
    pub session_lock: Option<LockConfiguration>,
    pub renderer: RendererSetup,
    pub run_end: RunEnd,
    pub preference_overrides: PreferenceOverrides,
    pub input_filter: Option<InputFilter>,
    pub observers: Observers,
    pub watches: Watches,
    pub clipboard: ClipboardTransfers,
    pub action_queue: ActionQueue,
    pub drags: Drags,
}

impl RuntimeSetup {
    pub fn panels(&self) -> impl Iterator<Item = &LayerSurface> {
        self.surfaces.iter().filter_map(|surface| match surface {
            SurfaceSetup::Panel(layer, _) => Some(layer),
            SurfaceSetup::Window(..) => None,
        })
    }

    pub fn has_surfaces(&self) -> bool {
        !self.surfaces.is_empty() || self.panels_per_output.is_some() || self.session_lock.is_some()
    }
}

#[derive(thiserror::Error)]
pub enum RunError<P: RenderedPlatform> {
    #[error("the app has not added a window, a layer or a session lock")]
    NoSurfaces,
    #[error(
        "the platform does not support what the app uses: {}. Run the app on a platform that \
         supports it, or remove the parts of the app that use it.",
        registry::Listed(.0)
    )]
    Unsupported(BTreeSet<Capability>),
    #[error(transparent)]
    Platform(P::Error),
    #[error("the thread of the platform could not be started")]
    SpawnPlatform(#[source] io::Error),
    #[error("the thread of the platform panicked")]
    PlatformPanicked,
    #[error("the renderer could not start")]
    StartRenderer(#[source] SpawnError<PlatformRendererError<P>>),
    #[error("the renderer failed")]
    RunRenderer(#[source] JoinError<PlatformRendererError<P>>),
    #[error("the shaper that measures text for the views could not be created")]
    CreateShaper(#[source] lavashell_renderer::Error),
    #[error("the choreographer or the scene builder failed")]
    SceneFailed(#[source] Arc<scene_pipeline::Error>),
    #[error(transparent)]
    View(#[from] ViewError),
}

impl<P: RenderedPlatform> fmt::Debug for RunError<P> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (name, field): (&str, Option<&dyn fmt::Debug>) = match self {
            Self::NoSurfaces => ("NoSurfaces", None),
            Self::Unsupported(missing) => ("Unsupported", Some(missing)),
            Self::Platform(error) => ("Platform", Some(error)),
            Self::SpawnPlatform(error) => ("SpawnPlatform", Some(error)),
            Self::PlatformPanicked => ("PlatformPanicked", None),
            Self::StartRenderer(error) => ("StartRenderer", Some(error)),
            Self::RunRenderer(error) => ("RunRenderer", Some(error)),
            Self::CreateShaper(error) => ("CreateShaper", Some(error)),
            Self::SceneFailed(error) => ("SceneFailed", Some(error)),
            Self::View(error) => ("View", Some(error)),
        };
        (field.into_iter())
            .fold(&mut formatter.debug_tuple(name), |tuple, field| {
                tuple.field(field)
            })
            .finish()
    }
}

pub fn run<P: RenderedPlatform>(app: App, platform_setup: P::Setup) -> Result<(), RunError<P>> {
    logging::install_default_logger();
    let setup = app.into_setup();
    if !setup.has_surfaces() {
        return Err(RunError::NoSurfaces);
    }
    drive::<P>(setup, platform_setup)
}

pub(crate) fn drive<P: RenderedPlatform>(
    setup: RuntimeSetup,
    platform_setup: P::Setup,
) -> Result<(), RunError<P>> {
    let connection = P::connect(platform_setup).map_err(RunError::Platform)?;
    let clock = connection.clock;
    let offered = connection.platform.capabilities();
    let missing = registry::missing(&setup, &offered);
    if !missing.is_empty() {
        return Err(RunError::Unsupported(missing));
    }
    let (inbox, inputs) = mailbox_on(clock.as_ref());
    let inputs =
        DeadlineReceiver::new(inputs, clock.as_ref()).map_err(ViewError::DeadlineReceiver)?;
    let (requests, platform_requests) = mailbox();
    let connected = Connected {
        backend: connection.backend,
        host: connection.host,
        clock: Arc::clone(&clock),
    };
    let mut view = ViewActor::<P>::start(setup, &offered, connected, requests, &inbox)?;
    let reports = (view.reports(inbox.outlet(ViewInput::Platform))).map_err(ViewError::from)?;
    let links = PlatformLinks {
        reports,
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
    drop(inputs);
    let renderer = stops.renderer.map_err(RunError::RunRenderer);
    let scheduler = stops
        .scheduler
        .map_err(|error| RunError::View(ViewError::SchedulerActor(error)));
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
