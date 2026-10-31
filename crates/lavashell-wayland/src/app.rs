// SPDX-License-Identifier: GPL-3.0-only
use crate::clipboard::{Clipboard, ClipboardState};
use lavashell_core::prelude::*;
use lavashell_core::scene::tree::ApplyError;
use lavashell_core::surface::SurfaceId;
use lavashell_core::view::{Failures, FrameError};
use lavashell_platform::{SurfaceEvent, SurfaceRequest};
use lavashell_renderer::{Fonts, FrameOutcome, Painter, VulkanRenderer, WaylandTarget};
use lavashell_runtime::{RunEnd, SurfaceRecords};
use sctk::compositor::{CompositorHandler, CompositorState, FrameCallbackData};
use sctk::output::{OutputHandler, OutputState};
use sctk::reexports::calloop::{EventLoop, LoopHandle, RegistrationToken};
use sctk::reexports::client::protocol::wl_callback::WlCallback;
use sctk::reexports::client::protocol::{
    wl_keyboard, wl_output, wl_pointer, wl_seat, wl_surface, wl_touch,
};
use sctk::reexports::client::{
    Connection, Dispatch, EventQueue, Proxy, QueueHandle, delegate_noop,
    globals::{GlobalList, registry_queue_init},
};
use sctk::reexports::protocols::ext::background_effect::v1::client::{
    ext_background_effect_manager_v1::ExtBackgroundEffectManagerV1,
    ext_background_effect_surface_v1::ExtBackgroundEffectSurfaceV1,
};
use sctk::reexports::protocols::wp::presentation_time::client::wp_presentation_feedback::WpPresentationFeedback;
use sctk::reexports::protocols::wp::{
    fractional_scale::v1::client::{
        wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1,
        wp_fractional_scale_v1::{self, WpFractionalScaleV1},
    },
    viewporter::client::{wp_viewport::WpViewport, wp_viewporter::WpViewporter},
};
use sctk::reexports::protocols_wlr::layer_shell::v1::client::{
    zwlr_layer_shell_v1::{self, ZwlrLayerShellV1},
    zwlr_layer_surface_v1::{self, ZwlrLayerSurfaceV1},
};
use sctk::registry::{ProvidesRegistryState, RegistryState};
use sctk::seat::keyboard::{KeyEvent, KeyboardHandler, Keysym, Modifiers, RawModifiers};
use sctk::seat::pointer::{
    CursorIcon, PointerEvent, PointerEventKind, PointerHandler, ThemeSpec, ThemedPointer,
};
use sctk::seat::{Capability, SeatHandler, SeatState};
use sctk::shell::WaylandSurface;
use sctk::shell::xdg::{
    XdgShell,
    window::{Window as NativeWindow, WindowConfigure, WindowDecorations, WindowHandler},
};
use sctk::shm::{Shm, ShmHandler};
use smithay_client_toolkit as sctk;
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use crate::surface::{LayerLevel, LayerSurface, Window};

mod accessibility;
mod creation;
mod focus_grab;
mod input;
mod key;
mod layer;
pub(crate) mod output_layers;
mod owned_wl_surface;
mod preferences;
mod presentation;
mod protocols;
mod public;
mod render;
mod scene;
pub(crate) mod session_lock;
mod shape;
mod standard_error;
mod startup;
mod touch;
mod transfer;
mod view_actions;
mod wake;
mod window_input;

pub use input::{HostKey, KeyboardFocus};
pub use presentation::{FrameReport, FrameStage, PresentationFlags, Submission};
pub use view_actions::ViewActions;

type PainterSetup = Box<dyn FnOnce(&mut Painter)>;
type InputFilter = Box<dyn FnMut(SurfaceId, InputEvent) -> Option<InputEvent>>;
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
    pub(crate) config: AppConfig,
    platform: PlatformFacts,
}

/// The callbacks and settings registered for an app before it runs. The Wayland platform calls and reads them while the
/// app runs.
#[derive(Default)]
pub(crate) struct AppConfig {
    input_filter: Option<InputFilter>,
    tick: Option<Box<dyn FnMut(Duration)>>,
    on_frame: Option<Box<dyn FnMut(FrameTime)>>,
    observer: Option<Box<dyn FnMut(FrameReport)>>,
    key_observer: Option<Box<dyn FnMut(HostKey) -> bool>>,
    focus_observer: Option<Box<dyn FnMut(KeyboardFocus)>>,
    view_observer: Option<ViewObserver>,
    pub(crate) view_actions: ViewActions,
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
    scene_generation: u64,
    target: Option<WaylandTarget>,
    tree: SceneTree,
    damage: DamageTracker,
    /// The role of the native surface until it retires.
    role: Option<Role>,
    wl: wl_surface::WlSurface,
    window: Option<std::sync::Arc<owned_wl_surface::OwnedWlSurface>>,
    wp_viewport: Option<WpViewport>,
    fractional: Option<WpFractionalScaleV1>,
    pointer_owner: Option<wl_pointer::WlPointer>,
    /// The frame callback requested for the next commit, until a shown frame commits it.
    frame_request: Option<WlCallback>,
    /// The frame callback committed with the last shown frame, until the compositor sends its `done` event.
    frame_callback: Option<WlCallback>,
    /// The presentation feedback of each submitted frame, until the compositor reports whether the frame was presented
    /// or discarded.
    feedback: Vec<WpPresentationFeedback>,
    /// The platform facts supplied by the app for the view, before the host replaced the facts known to the host.
    supplied_facts: PlatformFacts,
    clock: FrameClock,
    telemetry: PresentationTracker,
    output: Option<OutputInfo>,
    background: Option<ExtBackgroundEffectSurfaceV1>,
    blur_size: Option<Size>,
    shape: Option<shape::SurfaceShape>,
    drag_region: Option<crate::surface::DragRegion>,
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
        self.tree
            .next_change(shown)
            .is_some_and(|change| change <= shown)
    }

    /// Runs `frame` to apply the next frame of `view` to the scene tree, and logs each component that failed in it
    /// as an error. When the tree fails to apply an update, the view empties the tree and sends the whole scene with
    /// the next frame of the view. If the tree showed a scene before, that failure is logged and the frame ends
    /// without an error.
    /// If the tree was empty, the failure ends the frame with an error.
    fn show(
        &mut self,
        view: &mut View,
        frame: impl FnOnce(&mut View, &mut SceneTree) -> Result<Failures, FrameError>,
    ) -> Result<(), crate::Error> {
        let showed_nothing = self.tree.is_empty();
        match frame(view, &mut self.tree) {
            Ok(failures) => {
                crate::failures::log_failures(failures);
                Ok(())
            }
            Err(FrameError::Apply(error)) if !showed_nothing => {
                self.restart_tree(error);
                Ok(())
            }
            Err(error) => Err(crate::Error::ShowScene(error)),
        }
    }

    /// Handles an update that the tree failed to apply. The view has emptied the tree, so the damage tracker is
    /// invalidated and the next presentation paints the whole surface.
    fn restart_tree(&mut self, error: ApplyError) {
        log::warn!(
            "a surface could not apply a scene update and shows the whole scene next: {error}"
        );
        self.damage.invalidate();
    }

    fn wakes(&self, view: &View, now: Duration) -> Option<Duration> {
        let shown = self.clock.predict(now);
        wake::next_wake(
            now,
            shown,
            view.next_deadline(),
            self.tree.next_change(shown),
        )
    }
}

/// The size in buffer pixels of a surface in `viewport`.
fn physical_size(viewport: Viewport) -> Result<(u32, u32), crate::Error> {
    (viewport.size.buffer_size(viewport.density)).map_err(crate::Error::BufferSize)
}

impl Surface {
    fn retire(&mut self) {
        let Some(role) = self.role.take() else {
            return;
        };
        // The Vulkan target drops before the wl_surface is destroyed, because its swapchain presents to that
        // wl_surface. Dropping the swapchain waits until its queue is idle.
        self.target = None;
        self.accessibility = None;
        if let Some(background) = self.background.take() {
            background.destroy();
        }
        if let Some(fractional) = self.fractional.take() {
            fractional.destroy();
        }
        if let Some(wp_viewport) = self.wp_viewport.take() {
            wp_viewport.destroy();
        }
        match role {
            Role::Layer(layer) if layer.proxy.is_alive() => layer.proxy.destroy(),
            Role::Lock(surface) if surface.is_alive() => surface.destroy(),
            _ => {}
        }
        self.window = None;
        // The retired surface remains in `surfaces`, so a drag completion or a drop that arrives later still finds it.
        self.tree = SceneTree::default();
        self.damage.invalidate();
        self.shape = None;
        self.drag_region = None;
        self.pointer_owner = None;
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        self.retire();
    }
}

struct SeatDevices {
    seat: wl_seat::WlSeat,
    pointer: Option<ThemedPointer>,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    touch: Option<wl_touch::WlTouch>,
    touches: touch::TouchPoints,
    focus: Option<SurfaceId>,
    modifiers: Modifiers,
}

impl SeatDevices {
    fn new(seat: wl_seat::WlSeat) -> Self {
        Self {
            touches: touch::TouchPoints::new(seat.id().protocol_id()),
            seat,
            pointer: None,
            keyboard: None,
            touch: None,
            focus: None,
            modifiers: Modifiers::default(),
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
    view_configuration: preferences::ViewConfiguration,
    output_layers: Option<output_layers::Runtime>,
    accessibility_events: sctk::reexports::calloop::channel::Sender<crate::accessibility::Event>,
    session_lock: Option<session_lock::Runtime>,
    sources: Vec<RegistrationToken>,
    readable_sources: Vec<view_actions::ReadableSource>,
    // `drop` releases the native surfaces first. `surface_records` is declared before `paint` and `paint` before `gpu`,
    // so the views drop after the native surfaces and before the painter, and the GPU images and query pools cached by
    // the painter drop before the Vulkan device.
    surface_records: SurfaceRecords,
    /// The requests made by the runtime and executed by the next render.
    surface_requests: Vec<SurfaceRequest>,
    paint: Painter,
    surfaces: BTreeMap<SurfaceId, Surface>,
    scene: Option<scene::SceneRuntime>,
    gpu: VulkanRenderer,
    connection: Connection,
    pub(super) qh: QueueHandle<Self>,
    registry: RegistryState,
    seats: SeatState,
    outputs: OutputState,
    shm: Shm,
    compositor: CompositorState,
    protocols: SurfaceProtocols,
    devices: Vec<SeatDevices>,
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
        self.config.view_actions.close();
        self.shutdown_transfers();
        self.paint.retire_gpu_resources();
        self.surfaces.clear();
        self.scene = None;
        for source in self.sources.drain(..) {
            self.loop_handle.remove(source);
        }
        for source in self.readable_sources.drain(..) {
            self.loop_handle.remove(source.token);
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

    /// Executes the requests made by the runtime. A destroyed surface retires in the runtime only after its native
    /// surface released everything that presented to it.
    fn execute_surface_requests(&mut self) {
        for request in std::mem::take(&mut self.surface_requests) {
            match request {
                SurfaceRequest::Destroy(id) => {
                    self.paint.forget_effect_surface(id);
                    if let Some(surface) = self.surfaces.get_mut(&id) {
                        surface.retire();
                    }
                    self.surface_records.retire(id);
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
