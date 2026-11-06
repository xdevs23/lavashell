// SPDX-License-Identifier: GPL-3.0-only
use crate::clipboard::ClipboardState;
use crate::surface::LayerSurface;
use lavashell_actor::Closed;
use lavashell_core::frame::{FrameReport, FrameTime};
use lavashell_core::geometry::{Point, Viewport};
use lavashell_core::output::OutputInfo;
use lavashell_core::platform::PlatformFacts;
use lavashell_core::surface::{SurfaceChange, SurfaceId};
use lavashell_core::view::{Failures, FrameError, View};
use lavashell_platform::{AccessibilityEvent, SurfaceEvent, SurfaceRequest, WatchId};
use lavashell_renderer::text::Shaper;
use lavashell_renderer::{Fonts, PainterSetup, SceneSetup, SurfaceContents};
use lavashell_runtime::{
    ActionQueue, BuiltSurface, ClipboardTransfers, Drags, FrameKind, InputFilter, InputRouter,
    LockConfiguration, OutputRecords, PanelRecords, PanelRequests, PanelsPerOutput, Renderer,
    RendererInput, RendererStopped, RunEnd, ScenePipeline, SceneSink, SchedulerInput,
    SchedulerThread, SurfaceRecords, SurfaceShape, Watches, Window,
};
use sctk::compositor::CompositorState;
use sctk::output::OutputState;
use sctk::reexports::calloop::{LoopHandle, RegistrationToken};
use sctk::reexports::client::protocol::{wl_keyboard, wl_seat, wl_surface, wl_touch};
use sctk::reexports::client::{Connection, Proxy, QueueHandle};
use sctk::reexports::protocols::ext::background_effect::v1::client::{
    ext_background_effect_manager_v1::ExtBackgroundEffectManagerV1,
    ext_background_effect_surface_v1::ExtBackgroundEffectSurfaceV1,
};
use sctk::reexports::protocols::wp::{
    fractional_scale::v1::client::{
        wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1,
        wp_fractional_scale_v1::WpFractionalScaleV1,
    },
    viewporter::client::{wp_viewport::WpViewport, wp_viewporter::WpViewporter},
};
use sctk::reexports::protocols_wlr::layer_shell::v1::client::zwlr_layer_shell_v1::ZwlrLayerShellV1;
use sctk::registry::RegistryState;
use sctk::seat::SeatState;
use sctk::seat::pointer::ThemedPointer;
use sctk::shell::xdg::{XdgShell, window::Window as NativeWindow};
use sctk::shm::Shm;
use smithay_client_toolkit as sctk;
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

mod accessibility;
mod actor_link;
mod control;
mod creation;
mod device;
mod focus_grab;
mod input;
mod key;
mod layer;
pub(crate) mod output_layers;
mod outputs;
mod owned_wl_surface;
mod preferences;
mod presentation;
mod protocols;
mod public;
mod render;
mod renderer_outputs;
mod scene_reports;
mod scheduler_outputs;
mod session_lock;
mod shape;
mod standard_error;
mod startup;
mod touch;
mod transfer;
mod watch;
mod window_input;
#[cfg(test)]
mod wire_messages;

pub use input::{HostKey, KeyboardFocus};

type ViewObserver = Box<dyn FnMut(SurfaceId, Viewport, &View)>;

enum Request {
    Window(Window, View),
    Layer(LayerSurface, View),
}

pub struct App {
    requests: Vec<Request>,
    output_layers: Option<PanelsPerOutput<LayerSurface>>,
    fonts: Fonts,
    session_lock: Option<LockConfiguration>,
    run_end: RunEnd,
    clipboard: ClipboardTransfers,
    painter_setup: Option<PainterSetup>,
    scene: Option<SceneSetup>,
    gpu_timing: bool,
    preference_overrides: lavashell_core::preferences::PreferenceOverrides,
    input_filter: Option<InputFilter>,
    pub(crate) config: AppConfig,
    platform: PlatformFacts,
}

/// The callbacks and settings registered for an app before it runs. The Wayland platform calls and reads them while the
/// app runs.
pub(crate) struct AppConfig {
    tick: Option<Box<dyn FnMut(Duration)>>,
    on_frame: Option<Box<dyn FnMut(FrameTime)>>,
    observer: Option<Box<dyn FnMut(FrameReport)>>,
    key_observer: Option<Box<dyn FnMut(HostKey) -> bool>>,
    focus_observer: Option<Box<dyn FnMut(KeyboardFocus)>>,
    view_observer: Option<ViewObserver>,
    pub(crate) watches: Watches,
    pub(crate) action_queue: ActionQueue,
    pub(crate) drags: Drags,
    output_observer: Option<Box<dyn FnMut(SurfaceId, OutputInfo)>>,
    ui_budget: Option<Duration>,
    full_redraw: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        let watches = Watches::default();
        Self {
            tick: None,
            on_frame: None,
            observer: None,
            key_observer: None,
            focus_observer: None,
            view_observer: None,
            action_queue: ActionQueue::new(&watches),
            drags: Drags::new(&watches),
            watches,
            output_observer: None,
            ui_budget: None,
            full_redraw: false,
        }
    }
}

enum Role {
    Window { window: NativeWindow },
    Layer(Box<layer::Runtime>),
    Lock(session_lock::LockSurface),
}

impl Role {
    fn destroy(&self) {
        match self {
            Self::Layer(layer) if layer.proxy.is_alive() => layer.proxy.destroy(),
            Self::Lock(surface) if surface.is_alive() => surface.destroy(),
            _ => {}
        }
    }
}

struct Surface {
    accessibility: Option<crate::accessibility::Bridge>,
    frame: render::FrameProgress,
    last_frame: Option<LastFrame>,
    /// The panel requests applied by the commit without a frame that the renderer has not reported yet.
    committing: Option<PanelRequests>,
    role: Role,
    wl: wl_surface::WlSurface,
    /// Destroys the `wl_surface` when it drops. It is declared after `role`, so the role is destroyed first.
    _window: std::sync::Arc<owned_wl_surface::OwnedWlSurface>,
    viewport: Option<WpViewport>,
    fractional: Option<WpFractionalScaleV1>,
    output: Option<OutputInfo>,
    background: Option<ExtBackgroundEffectSurfaceV1>,
    shape: SurfaceShape,
}

impl Surface {
    fn needs_visual_frame(&self, view: &View) -> bool {
        view.needs_visual_frame() || self.shape.needs_frame()
    }

    /// Whether the target of the surface contains the frame drawn last at `viewport`.
    fn contents(&self, viewport: Viewport) -> SurfaceContents {
        match self.last_frame {
            Some(last) if last.viewport == viewport => last.contents,
            _ => SurfaceContents::Outdated,
        }
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        self.accessibility = None;
        destroy_objects(
            self.viewport.take(),
            self.fractional.take(),
            self.background.take(),
            Some(&self.role),
        );
    }
}

/// The viewport of the frame drawn last for a surface, and whether the target of the surface contains that frame.
#[derive(Clone, Copy)]
struct LastFrame {
    viewport: Viewport,
    contents: SurfaceContents,
}

/// Which actors stopped while the platform dispatched events, so that the next render ends the run.
#[derive(Default)]
struct Stops {
    scene: bool,
    scheduler: bool,
    renderer: bool,
}

impl Stops {
    /// Records the stop reported by `error`, or logs `error` when it reports another failure.
    fn record(&mut self, error: crate::Error) {
        match error {
            crate::Error::SceneStopped => self.scene = true,
            crate::Error::SchedulerStopped => self.scheduler = true,
            crate::Error::RendererStopped(_) => self.renderer = true,
            error => log::error!("{error}"),
        }
    }

    /// Fails with the stop of the scene pipeline, the frame scheduler or the renderer, when one was recorded.
    fn check(&self) -> Result<(), crate::Error> {
        if self.scene {
            Err(crate::Error::SceneStopped)
        } else if self.scheduler {
            Err(crate::Error::SchedulerStopped)
        } else if self.renderer {
            Err(crate::Error::RendererStopped(RendererStopped))
        } else {
            Ok(())
        }
    }
}

struct SeatDevices {
    seat: wl_seat::WlSeat,
    pointer: Option<ThemedPointer>,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    touch: Option<wl_touch::WlTouch>,
    /// The last position of each touch point that is down, because the protocol reports no position when a point goes
    /// up.
    touch_positions: BTreeMap<i32, Point>,
}

impl SeatDevices {
    fn new(seat: wl_seat::WlSeat) -> Self {
        Self {
            seat,
            pointer: None,
            keyboard: None,
            touch: None,
            touch_positions: BTreeMap::new(),
        }
    }
}

struct SurfaceProtocols {
    xdg: Option<XdgShell>,
    layers: Option<ZwlrLayerShellV1>,
    viewporter: Option<WpViewporter>,
    fractional: Option<WpFractionalScaleManagerV1>,
    background: Option<ExtBackgroundEffectManagerV1>,
}

pub(super) struct Platform {
    focus_grabs: Option<focus_grab::Grabs>,
    output_layers: Option<output_layers::Runtime>,
    accessibility_events: sctk::reexports::calloop::channel::Sender<AccessibilityEvent>,
    session_lock: Option<session_lock::Runtime>,
    sources: Vec<RegistrationToken>,
    /// The event loop source of each watched file descriptor.
    watches: BTreeMap<WatchId, RegistrationToken>,
    pub(super) surface_records: SurfaceRecords,
    panels: PanelRecords,
    /// The requests made by the runtime and executed by the next render.
    surface_requests: Vec<SurfaceRequest>,
    scene: ScenePipeline,
    renderer: Renderer<control::Control>,
    stops: Stops,
    /// Measures text for the views as the painter of the renderer shapes it.
    shaper: Shaper,
    /// The instant at which the scene source on the renderer thread produces the next image, as the renderer
    /// reported last.
    scene_deadline: Option<Instant>,
    scheduler: SchedulerThread,
    /// The time at which the scheduler asks the views to report again what they need.
    scheduler_wake: Option<Duration>,
    /// The newest frame started by the scheduler for each surface, until a render composes or draws it.
    started_frames: BTreeMap<SurfaceId, (FrameKind, FrameTime)>,
    /// The built state that the next render commits without a frame for each surface. The state is read when the
    /// frame scheduler starts the commit, while no frame of the surface is composed or drawn.
    state_commits: BTreeMap<SurfaceId, std::sync::Arc<BuiltSurface>>,
    surfaces: BTreeMap<SurfaceId, Surface>,
    /// The native surfaces destroyed by the runtime, until the renderer reports that nothing presents to them.
    retiring: BTreeMap<SurfaceId, Surface>,
    connection: Connection,
    pub(super) qh: QueueHandle<Self>,
    registry: RegistryState,
    seats: SeatState,
    outputs: OutputState,
    output_records: OutputRecords,
    shm: Shm,
    compositor: CompositorState,
    protocols: SurfaceProtocols,
    devices: Vec<SeatDevices>,
    input: InputRouter,
    /// The failure of a request that followed from an input, which ends the run once the event dispatch returns.
    input_failure: Option<crate::Error>,
    pub(super) loop_handle: LoopHandle<'static, Self>,
    pub(super) clipboard: ClipboardState,
    pub(super) config: AppConfig,
    presentation: Option<presentation::Presentation>,
    run_end: RunEnd,
    start: Instant,
    last_tick: Duration,
}

impl Drop for Platform {
    fn drop(&mut self) {
        self.config.action_queue.close();
        self.shutdown_transfers();
        self.config.watches.close();
        // Stopping the renderer drops every target and control, so nothing presents to the native surfaces anymore.
        if let Err(error) = self.renderer.stop() {
            log::error!("{}", lavashell_core::error::message_chain(error));
        }
        self.surfaces.clear();
        self.retiring.clear();
        for source in self.sources.drain(..) {
            self.loop_handle.remove(source);
        }
        for token in std::mem::take(&mut self.watches).into_values() {
            self.loop_handle.remove(token);
        }
    }
}

impl Platform {
    /// The id of the native surface `wl`.
    fn find_surface(&self, wl: &wl_surface::WlSurface) -> Option<SurfaceId> {
        (self.surfaces.iter()).find_map(|(id, known)| (&known.wl == wl).then_some(*id))
    }

    /// Hands `event` for `surface` to the runtime, and queues the request that follows from it.
    fn surface_event(&mut self, surface: SurfaceId, event: SurfaceEvent) {
        (self.surface_requests).extend(self.surface_records.handle(surface, event));
    }

    /// Sends `input` to the frame scheduler. A scheduler that is gone closes its outputs, which ends the run.
    fn schedule(&self, input: SchedulerInput) {
        let _gone_scheduler_ends_the_run = self.scheduler.send(input);
    }

    /// Executes the requests made by the runtime. A destroyed surface retires in the renderer first. The native surface
    /// and the record of that surface retire once the renderer reports that nothing presents to the surface.
    fn execute_surface_requests(&mut self) -> Result<(), crate::Error> {
        for request in std::mem::take(&mut self.surface_requests) {
            match request {
                SurfaceRequest::Destroy(id) => {
                    self.schedule(SchedulerInput::Closed(id));
                    let _undrawn_frame = self.started_frames.remove(&id);
                    self.config
                        .drags
                        .forget_surface(id, &mut self.surface_records);
                    self.output_records.forget(id);
                    self.input.forget_surface(id);
                    self.panels.forget(id);
                    self.forget_focus_grab(id)?;
                    self.config.action_queue.forget_surface(id);
                    self.clipboard.transfers.forget_surface(id);
                    if let Some(surface) = self.surfaces.remove(&id) {
                        let _replaced = self.retiring.insert(id, surface);
                    }
                    self.renderer.send(RendererInput::Retire(id))?;
                }
            }
        }
        Ok(())
    }
}

/// Sends `changes` and the geometry of the panel of surface `id` to `sink`, and ends the frame as
/// [`SceneSink::end_state_frame`] does. Returns whether the frame ended.
fn send_surface_state(
    mut sink: SceneSink<'_>,
    panels: &PanelRecords,
    id: SurfaceId,
    changes: impl IntoIterator<Item = SurfaceChange>,
) -> Result<bool, Closed> {
    for change in changes.into_iter().chain(panels.geometry_change(id)) {
        sink.set_state(change)?;
    }
    sink.end_state_frame()
}

/// Destroys the protocol objects of a native surface except the `wl_surface`.
fn destroy_objects(
    viewport: Option<WpViewport>,
    fractional: Option<WpFractionalScaleV1>,
    background: Option<ExtBackgroundEffectSurfaceV1>,
    role: Option<&Role>,
) {
    if let Some(viewport) = viewport {
        viewport.destroy();
    }
    if let Some(background) = background {
        background.destroy();
    }
    if let Some(fractional) = fractional {
        fractional.destroy();
    }
    if let Some(role) = role {
        role.destroy();
    }
}

/// Runs `frame` to send the next frame of `view` into `sink`, and logs each component that failed in the frame as an
/// error. A failed frame ends the run when no earlier frame ended into the sink, because the surface then has no scene
/// to keep showing. Otherwise the failure is logged, and the view sends the whole scene with the next frame.
fn send_frame(
    view: &mut View,
    mut sink: SceneSink<'_>,
    frame: impl FnOnce(&mut View, &mut SceneSink<'_>) -> Result<Failures, FrameError<Closed>>,
) -> Result<(), crate::Error> {
    let shows_scene = sink.any_frame_ended();
    match frame(view, &mut sink) {
        Ok(failures) => {
            crate::failures::log_failures(failures);
            Ok(())
        }
        Err(FrameError::Sink(Closed)) => Err(crate::Error::SceneStopped),
        Err(error) if shows_scene => {
            warn_whole_scene_next(&error);
            Ok(())
        }
        Err(error) => Err(crate::Error::ShowScene(error)),
    }
}

/// Logs `error`, after which the view sends the whole scene with the next frame.
fn warn_whole_scene_next(error: impl std::error::Error) {
    log::warn!(
        "a frame failed, and the whole scene is sent with the next frame: {}",
        lavashell_core::error::message_chain(error)
    );
}

/// The native surface `id`, its view and its viewport, while the surface is open.
fn open_surface<'a>(
    surfaces: &'a mut BTreeMap<SurfaceId, Surface>,
    records: &'a mut SurfaceRecords,
    id: SurfaceId,
) -> Result<(&'a mut Surface, &'a mut View, Viewport), crate::Error> {
    let viewport = records.open_viewport(id);
    let found = (surfaces.get_mut(&id))
        .zip(records.open_view_mut(id))
        .zip(viewport);
    let ((surface, view), viewport) = found.ok_or(crate::Error::SurfaceNotOpen(id))?;
    Ok((surface, view, viewport))
}

/// The configure event for the lengths in logical pixels that the compositor sets. A length that the compositor
/// leaves to the surface is `None`.
fn configure_event(width: Option<u32>, height: Option<u32>) -> SurfaceEvent {
    let length = |pixels: u32| crate::logical::length(f64::from(pixels));
    SurfaceEvent::Configured {
        width: width.map(length),
        height: height.map(length),
    }
}
