// SPDX-License-Identifier: GPL-3.0-only
use crate::clipboard::{Clipboard, ClipboardState};
use lavashell_core::prelude::*;
use lavashell_core::producer::NamespaceProducer;
use lavashell_core::scene::tree::ApplyError;
use lavashell_core::surface::SurfaceId;
use lavashell_core::view::FrameError;
use lavashell_renderer::{Fonts, FrameOutcome, Painter, VulkanRenderer, WaylandTarget};
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
use std::time::{Duration, Instant};

use crate::surface::{LayerLevel, LayerSurface, Window};

mod accessibility;
mod creation;
mod focus_grab;
mod input;
mod key;
mod layer;
mod lifecycle;
mod namespaces;
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
pub use namespaces::SurfaceStart;
pub use presentation::{FrameReport, FrameStage, PresentationFlags, Submission};
pub use view_actions::ViewActions;

type PainterSetup = Box<dyn FnOnce(&mut Painter)>;
type InputFilter = Box<dyn FnMut(usize, InputEvent) -> Option<InputEvent>>;
type ViewObserver = Box<dyn FnMut(SurfaceId, Viewport, &View)>;

/// Makes the view of a requested surface with the scene of the surface, before its first frame.
type ViewStart = Box<dyn FnOnce(&mut SurfaceStart<'_>) -> View>;

enum Request {
    Window(Window, ViewStart),
    Layer(LayerSurface, ViewStart),
    /// A lock surface on the output, whose view the session lock starts.
    Lock(wl_output::WlOutput),
}

pub struct App {
    requests: Vec<Request>,
    output_layers: Option<output_layers::Configuration>,
    fonts: Fonts,
    session_lock: Option<session_lock::Configuration>,
    should_exit: Option<Box<dyn FnMut() -> bool>>,
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
    exit_after: Option<Duration>,
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
    output_observer: Option<Box<dyn FnMut(usize, OutputInfo)>>,
    ui_budget: Option<Duration>,
    full_redraw: bool,
}

enum Role {
    Window { window: NativeWindow },
    Layer(Box<layer::Runtime>),
    Lock(session_lock::LockSurface),
}

struct Surface {
    /// The ID of the surface in the painter and the GPU renderer, the same in every frame.
    id: SurfaceId,
    accessibility: Option<crate::accessibility::Bridge>,
    scene_generation: u64,
    target: Option<WaylandTarget>,
    tree: SceneTree,
    producers: Vec<Box<dyn NamespaceProducer>>,
    damage: DamageTracker,
    lifecycle: lifecycle::Lifecycle,
    wl: wl_surface::WlSurface,
    window: Option<std::sync::Arc<owned_wl_surface::OwnedWlSurface>>,
    wp_viewport: Option<WpViewport>,
    fractional: Option<WpFractionalScaleV1>,
    view: View,
    pointer_owner: Option<wl_pointer::WlPointer>,
    size: (u32, u32),
    density: Density,
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
    fn needs_visual_frame(&self) -> bool {
        self.view.needs_visual_frame()
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

    /// Runs `frame` to apply the next frame of the view to the scene tree, as `namespaces::apply_frame` does. A
    /// failure to apply that the view recovers from is logged, and the frame ends without an error.
    fn show(
        &mut self,
        frame: impl FnOnce(&mut View, &mut SceneTree) -> Result<(), FrameError>,
    ) -> Result<(), crate::Error> {
        let view = &mut self.view;
        let restarted = namespaces::apply_frame(&mut self.tree, |tree| frame(view, tree));
        if let Some(error) = restarted.map_err(crate::Error::ShowScene)? {
            self.restart_tree(error);
        }
        Ok(())
    }

    /// Handles an update that the tree failed to apply. The view has emptied the root namespace of the tree, so the
    /// damage tracker is invalidated and the next presentation paints the whole surface.
    fn restart_tree(&mut self, error: ApplyError) {
        log::warn!(
            "a surface could not apply a scene update and shows the whole scene next: {error}"
        );
        self.damage.invalidate();
    }

    fn wakes(&self, now: Duration) -> Option<Duration> {
        let shown = self.clock.predict(now);
        wake::next_wake(
            now,
            shown,
            self.view.next_deadline(),
            self.tree.next_change(shown),
        )
    }

    fn logical_size(&self) -> Size {
        crate::logical::size(self.size)
    }

    fn physical_size(&self) -> Result<(u32, u32), crate::Error> {
        (self.logical_size().buffer_size(self.density)).map_err(crate::Error::BufferSize)
    }

    fn logical_viewport(&self) -> Viewport {
        Viewport::new(self.logical_size(), self.density)
    }
}

impl Surface {
    fn retire(&mut self) {
        let Some(role) = self.lifecycle.retire() else {
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
        // The retired surface stays in the list of surfaces, so its index continues to identify a closed surface.
        // Replacing the view with an empty view drops the application view and every resource in it, because those
        // resources must not outlive the native surface.
        self.view = View::new(self.view.platform(), |_| {});
        self.producers.clear();
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
    focus: Option<usize>,
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
    // `paint` is declared before `gpu`, so the GPU images and query pools cached by the painter drop before the
    // Vulkan device.
    paint: Painter,
    surfaces: Vec<Surface>,
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
    fn index(&self, surface: &wl_surface::WlSurface) -> Option<usize> {
        self.surfaces.iter().position(|known| &known.wl == surface)
    }
}
