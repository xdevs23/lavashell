// SPDX-License-Identifier: GPL-3.0-only
use crate::clipboard::{Clipboard, ClipboardState};
use crate::surface::LayerSurface;
use lavashell_core::frame::{FrameClock, FrameReport, FrameTime, PresentationTracker};
use lavashell_core::geometry::{Point, Size, Viewport};
use lavashell_core::output::OutputInfo;
use lavashell_core::platform::PlatformFacts;
use lavashell_core::scene::SceneBuilder;
use lavashell_core::surface::SurfaceId;
use lavashell_core::view::{Failures, FrameError, View};
use lavashell_platform::{AccessibilityEvent, SurfaceEvent, SurfaceRequest, WatchId};
use lavashell_renderer::{Fonts, PainterSetup, RendererState};
use lavashell_runtime::{
    ActionQueue, Choreographers, InputFilter, InputRouter, OutputRecords, RunEnd, SceneSink,
    SurfaceRecords, Window,
};
use sctk::compositor::CompositorState;
use sctk::output::OutputState;
use sctk::reexports::calloop::{LoopHandle, RegistrationToken};
use sctk::reexports::client::protocol::{wl_keyboard, wl_output, wl_seat, wl_surface, wl_touch};
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
use std::convert::Infallible;
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
pub(crate) mod session_lock;
mod shape;
mod standard_error;
mod startup;
mod touch;
mod transfer;
mod wake;
mod watch;
mod window_input;

pub use input::{HostKey, KeyboardFocus};

type ViewObserver = Box<dyn FnMut(SurfaceId, Viewport, &View)>;

enum Request {
    Window(Window, View),
    Layer(LayerSurface, View),
    Lock(wl_output::WlOutput, View),
}

pub struct App {
    requests: Vec<Request>,
    output_layers: Option<output_layers::Configuration>,
    fonts: Fonts,
    session_lock: Option<session_lock::Configuration>,
    run_end: RunEnd,
    clipboard: Clipboard,
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
#[derive(Default)]
pub(crate) struct AppConfig {
    tick: Option<Box<dyn FnMut(Duration)>>,
    on_frame: Option<Box<dyn FnMut(FrameTime)>>,
    observer: Option<Box<dyn FnMut(FrameReport)>>,
    key_observer: Option<Box<dyn FnMut(HostKey) -> bool>>,
    focus_observer: Option<Box<dyn FnMut(KeyboardFocus)>>,
    view_observer: Option<ViewObserver>,
    pub(crate) action_queue: ActionQueue,
    drag_observer: Option<transfer::DragObserver>,
    drop_observer: Option<transfer::DropObserver>,
    drag_source_observer: Option<transfer::DragSourceObserver>,
    pub(crate) text_transfers: Option<crate::text_transfer::TextTransfers>,
    output_observer: Option<Box<dyn FnMut(SurfaceId, OutputInfo)>>,
    ui_budget: Option<Duration>,
    full_redraw: bool,
}

enum Role {
    Window { window: NativeWindow },
    Layer(Box<layer::Runtime>),
    Lock(session_lock::LockSurface),
}

struct Surface {
    accessibility: Option<crate::accessibility::Bridge>,
    builder: SceneBuilder,
    role: Role,
    wl: wl_surface::WlSurface,
    /// Destroys the `wl_surface` when it drops. It is declared after `role`, so the role is destroyed first.
    _window: std::sync::Arc<owned_wl_surface::OwnedWlSurface>,
    fractional: Option<WpFractionalScaleV1>,
    /// Whether the compositor has not sent the `done` event of the frame callback committed with the last shown frame.
    frame_callback_pending: bool,
    clock: FrameClock,
    telemetry: PresentationTracker,
    output: Option<OutputInfo>,
    background: Option<ExtBackgroundEffectSurfaceV1>,
    blur_size: Option<Size>,
    shape: Option<shape::SurfaceShape>,
}

impl Surface {
    fn needs_visual_frame(&self, view: &View) -> bool {
        view.needs_visual_frame()
            || self
                .shape
                .as_ref()
                .is_some_and(shape::SurfaceShape::needs_frame)
    }

    fn moving(&self, now: Duration) -> bool {
        let shown = self.clock.predict(now);
        self.builder
            .next_change(shown)
            .is_some_and(|change| change <= shown)
    }

    /// Runs `frame` to send the next frame of `view` to the choreographer for surface `id`, builds the frames merged
    /// there into the scene builder, and logs each component that failed in the frame as an error. When a change does
    /// not fit the scene, the view sends the whole scene with the next frame. The builder restarts with that frame.
    /// If the builder had a scene before, the failure is logged and the frame ends without error. If the builder was
    /// empty, the failure ends the frame with an error.
    fn show(
        &mut self,
        id: SurfaceId,
        view: &mut View,
        choreographers: &mut Choreographers,
        frame: impl FnOnce(&mut View, &mut SceneSink) -> Result<Failures, FrameError<Infallible>>,
    ) -> Result<(), crate::Error> {
        let showed_nothing = self.builder.is_empty();
        let error = match frame(view, choreographers.sink(id)) {
            Ok(failures) => {
                crate::failures::log_failures(failures);
                let Err(error) = choreographers.build(id, &mut self.builder) else {
                    return Ok(());
                };
                view.invalidate();
                crate::Error::BuildScene(error)
            }
            Err(error) => crate::Error::ShowScene(error),
        };
        if showed_nothing {
            return Err(error);
        }
        log::warn!(
            "the surface shows the whole scene next: {}",
            lavashell_core::error::message_chain(&error)
        );
        Ok(())
    }

    fn wakes(&self, view: &View, now: Duration) -> Option<Duration> {
        let shown = self.clock.predict(now);
        wake::next_wake(
            now,
            shown,
            view.next_deadline(),
            self.builder.next_change(shown),
        )
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        self.accessibility = None;
        if let Some(background) = self.background.take() {
            background.destroy();
        }
        if let Some(fractional) = self.fractional.take() {
            fractional.destroy();
        }
        match &self.role {
            Role::Layer(layer) if layer.proxy.is_alive() => layer.proxy.destroy(),
            Role::Lock(surface) if surface.is_alive() => surface.destroy(),
            _ => {}
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
    focus_grabs: focus_grab::Capability,
    output_layers: Option<output_layers::Runtime>,
    accessibility_events: sctk::reexports::calloop::channel::Sender<AccessibilityEvent>,
    session_lock: Option<session_lock::Runtime>,
    sources: Vec<RegistrationToken>,
    /// The event loop source of each watched file descriptor.
    watches: BTreeMap<WatchId, RegistrationToken>,
    // `drop` releases the native surfaces first. `surface_records` is declared before `renderer`, so the views drop
    // after the native surfaces and before the painter.
    surface_records: SurfaceRecords,
    /// The requests made by the runtime and executed by the next render.
    surface_requests: Vec<SurfaceRequest>,
    choreographers: Choreographers,
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

    /// Executes the requests made by the runtime. A destroyed surface is removed from the map of surfaces. It retires
    /// in the runtime only after the native surface has released everything that presented to it.
    fn execute_surface_requests(&mut self) {
        for request in std::mem::take(&mut self.surface_requests) {
            match request {
                SurfaceRequest::Destroy(id) => {
                    self.output_records.forget(id);
                    self.input.forget_surface(id);
                    detach(&mut self.renderer, id);
                    drop(self.surfaces.remove(&id));
                    self.choreographers.retire(id);
                    self.surface_records.retire(id);
                    self.config.action_queue.forget_surface(id);
                }
            }
        }
    }
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
