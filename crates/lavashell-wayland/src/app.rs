// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::geometry::Point;
use lavashell_core::platform::PlatformFacts;
use lavashell_core::preferences::PreferenceOverrides;
use lavashell_core::surface::ConfigureToken;
use lavashell_platform::SurfaceEvent;
use lavashell_renderer::{Fonts, PainterSetup, SceneSetup};
use lavashell_runtime::{
    ActionQueue, ClipboardTransfers, Drags, InputFilter, LayerSurface, LockConfiguration,
    Observers, PanelsPerOutput, RunEnd, SurfaceSetup, Watches,
};
use sctk::reexports::client::Proxy;
use sctk::reexports::client::protocol::{wl_keyboard, wl_seat, wl_surface, wl_touch};
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
use sctk::seat::pointer::ThemedPointer;
use sctk::shell::xdg::{XdgShell, window::Window as NativeWindow};
use smithay_client_toolkit as sctk;
use std::collections::BTreeMap;
use std::num::NonZero;
use std::time::Duration;

pub(crate) use control::Control;
pub(crate) use focus_grab::Grabs;
pub(crate) use outputs::advertised_outputs;
pub(crate) use preferences::install as install_preferences;
pub(crate) use presentation::Presentation;
pub(crate) use watch::WatchSources;

mod accessibility;
mod control;
mod creation;
mod device;
mod focus_grab;
mod input;
mod key;
mod layer;
mod output_layers;
mod outputs;
mod owned_wl_surface;
mod preferences;
mod presentation;
mod protocols;
mod public;
mod session_lock;
mod standard_error;
mod startup;
mod touch;
mod transfer;
mod watch;
mod window_input;
#[cfg(test)]
mod wire_messages;

pub struct App {
    surfaces: Vec<SurfaceSetup>,
    output_layers: Option<PanelsPerOutput<LayerSurface>>,
    fonts: Fonts,
    session_lock: Option<LockConfiguration>,
    run_end: RunEnd,
    painter_setup: Option<PainterSetup>,
    scene: Option<SceneSetup>,
    gpu_timing: bool,
    full_redraw: bool,
    ui_budget: Option<Duration>,
    preference_overrides: PreferenceOverrides,
    input_filter: Option<InputFilter>,
    observers: Observers,
    watches: Watches,
    clipboard: ClipboardTransfers,
    action_queue: ActionQueue,
    drags: Drags,
    platform: PlatformFacts,
}

enum NativeRole {
    Window { window: NativeWindow },
    Layer(Box<layer::Runtime>),
    Lock(session_lock::LockSurface),
}

impl NativeRole {
    fn destroy(&self) {
        match self {
            Self::Layer(layer) if layer.proxy.is_alive() => layer.proxy.destroy(),
            Self::Lock(surface) if surface.is_alive() => surface.destroy(),
            _ => {}
        }
    }
}

pub(crate) struct Surface {
    accessibility: Option<crate::accessibility::Bridge>,
    role: NativeRole,
    pub(crate) wl: wl_surface::WlSurface,
    /// Destroys the `wl_surface` when it drops. It is declared after `role`, so the role is destroyed first.
    _window: std::sync::Arc<owned_wl_surface::OwnedWlSurface>,
    viewport: Option<WpViewport>,
    fractional: Option<WpFractionalScaleV1>,
    background: Option<ExtBackgroundEffectSurfaceV1>,
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

pub(crate) struct SeatDevices {
    seat: wl_seat::WlSeat,
    pointer: Option<ThemedPointer>,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    touch: Option<wl_touch::WlTouch>,
    /// The last position of each touch point that is down, because the protocol reports no position when a point goes
    /// up.
    touch_positions: BTreeMap<i32, Point>,
}

impl SeatDevices {
    pub(crate) fn new(seat: wl_seat::WlSeat) -> Self {
        Self {
            seat,
            pointer: None,
            keyboard: None,
            touch: None,
            touch_positions: BTreeMap::new(),
        }
    }
}

pub(crate) struct SurfaceProtocols {
    xdg: Option<XdgShell>,
    layers: Option<ZwlrLayerShellV1>,
    viewporter: Option<WpViewporter>,
    fractional: Option<WpFractionalScaleManagerV1>,
    background: Option<ExtBackgroundEffectManagerV1>,
}

/// Destroys the protocol objects of a native surface except the `wl_surface`.
fn destroy_objects(
    viewport: Option<WpViewport>,
    fractional: Option<WpFractionalScaleV1>,
    background: Option<ExtBackgroundEffectSurfaceV1>,
    role: Option<&NativeRole>,
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

/// The configure event for the lengths in logical pixels that the compositor sets, with the `token` of a configure
/// that the frame applying it acknowledges. A length that the compositor leaves to the surface is `None`.
fn configure_event(
    width: Option<NonZero<u32>>,
    height: Option<NonZero<u32>>,
    token: Option<ConfigureToken>,
) -> SurfaceEvent {
    let length = |pixels: NonZero<u32>| crate::logical::length(f64::from(pixels.get()));
    SurfaceEvent::Configured {
        width: width.map(length),
        height: height.map(length),
        token,
    }
}
