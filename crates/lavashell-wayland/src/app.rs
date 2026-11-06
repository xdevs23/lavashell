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
use lavashell_renderer::{Fonts, PainterSetup, RendererState};
use lavashell_runtime::{
    ActionQueue, ClipboardTransfers, Drags, FrameKind, FrameScheduler, InputFilter, InputRouter,
    LockConfiguration, OutputRecords, PanelRecords, PanelsPerOutput, RunEnd, ScenePipeline,
    SceneSink, SchedulerInput, SchedulerOutput, SurfaceRecords, SurfaceShape, Watches, Window,
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
    viewporter::client::wp_viewporter::WpViewporter,
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
mod scene_reports;
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
    scene: Option<Box<dyn lavashell_renderer::vulkan::SceneSource>>,
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
    next_draw: render::NextDraw,
    /// Whether the scene builder built a surface state that the host receives with a commit without a frame.
    uncommitted_state: bool,
    role: Role,
    wl: wl_surface::WlSurface,
    /// Destroys the `wl_surface` when it drops. It is declared after `role`, so the role is destroyed first.
    _window: std::sync::Arc<owned_wl_surface::OwnedWlSurface>,
    fractional: Option<WpFractionalScaleV1>,
    output: Option<OutputInfo>,
    background: Option<ExtBackgroundEffectSurfaceV1>,
    shape: SurfaceShape,
}

impl Surface {
    fn needs_visual_frame(&self, view: &View) -> bool {
        view.needs_visual_frame() || self.shape.needs_frame()
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        self.accessibility = None;
        destroy_objects(
            self.fractional.take(),
            self.background.take(),
            Some(&self.role),
        );
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
    // `drop` releases the native surfaces first. `surface_records` is declared before `renderer`, so the views drop
    // after the native surfaces and before the painter.
    pub(super) surface_records: SurfaceRecords,
    panels: PanelRecords,
    /// The requests made by the runtime and executed by the next render.
    surface_requests: Vec<SurfaceRequest>,
    scene: ScenePipeline,
    /// Whether the scene builder stopped, so that the next render ends the run.
    scene_stopped: bool,
    scheduler: FrameScheduler,
    /// The newest frame started by the scheduler for each surface, until a render composes or draws it.
    started_frames: BTreeMap<SurfaceId, (FrameKind, FrameTime)>,
    surfaces: BTreeMap<SurfaceId, Surface>,
    renderer: RendererState<control::Control>,
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
        for &id in self.surfaces.keys() {
            detach(&mut self.renderer, id);
        }
        self.surfaces.clear();
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

    /// Hands `input` to the frame scheduler. Passes each report to the frame observer of the app at once, and keeps
    /// each started frame for the next render.
    fn schedule(&mut self, input: SchedulerInput) {
        let mut outputs = Vec::new();
        (self.scheduler).handle(self.start.elapsed(), input, &mut outputs);
        for output in outputs {
            match output {
                SchedulerOutput::Frame {
                    surface,
                    kind,
                    time,
                } => {
                    let _older_frame = self.started_frames.insert(surface, (kind, time));
                }
                SchedulerOutput::Report(report) => {
                    if let Some(observer) = &mut self.config.observer {
                        observer(*report);
                    }
                }
            }
        }
    }

    /// Executes the requests made by the runtime. A destroyed surface is removed from the map of surfaces. It retires
    /// in the runtime only after the native surface has released everything that presented to it.
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
                    detach(&mut self.renderer, id);
                    drop(self.surfaces.remove(&id));
                    if self.scene.retire(id).is_err() {
                        self.scene_stopped = true;
                    }
                    self.surface_records.retire(id);
                    self.config.action_queue.forget_surface(id);
                    self.clipboard.transfers.forget_surface(id);
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

/// Destroys the protocol objects of a native surface apart from its `wl_surface` and the objects of its control.
fn destroy_objects(
    fractional: Option<WpFractionalScaleV1>,
    background: Option<ExtBackgroundEffectSurfaceV1>,
    role: Option<&Role>,
) {
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

/// Drops the target of `surface` in `renderer` and destroys the objects of its control. The target drops before the
/// role objects of the surface are destroyed, because its swapchain presents to the surface. Dropping the swapchain
/// blocks until the queue used by it is idle.
fn detach(renderer: &mut RendererState<control::Control>, surface: SurfaceId) {
    if let Some(control) = renderer.retire(surface) {
        control.destroy();
    }
}
