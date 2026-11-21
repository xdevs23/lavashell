// SPDX-License-Identifier: GPL-3.0-only
use crate::accessibility::Bridge;
use crate::sctk;
use lavashell_core::surface::ConfigureToken;
use lavashell_core::units::Dp;
use lavashell_core::window::WindowState;
use lavashell_platform::SurfaceEvent;
use sctk::reexports::client::Proxy;
use sctk::reexports::client::protocol::wl_surface;
use sctk::reexports::protocols::ext::background_effect::v1::client::ext_background_effect_manager_v1;
use sctk::reexports::protocols::wp::{
    fractional_scale::v1::client::wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1,
    viewporter::client::wp_viewporter::WpViewporter,
};
use sctk::reexports::protocols_wlr::layer_shell::v1::client::{
    zwlr_layer_shell_v1::ZwlrLayerShellV1, zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
};
use sctk::shell::xdg::{XdgShell, window::Window as NativeWindow};
use std::num::NonZero;
use std::sync::Arc;

pub(crate) use control::{Control, RoleControl, SurfaceObjects, initial_role_state};
pub(crate) use device::{keyboard_device, pointer_device, pointer_event, protocol_serial};
pub(crate) use extensions::SurfaceExtensions;
pub(crate) use focus_grab::Grabs;
pub(crate) use input::{cursor_icon, is_primary_press};
pub(crate) use key::{key_event, key_repeat};
pub(crate) use layer::{LayerTarget, translate_configure};
pub(crate) use outputs::{advertised_outputs, monitor_info, output_id, refresh_interval};
pub(crate) use owned_wl_surface::OwnedWlSurface;
pub(crate) use preferences::report_user_languages;
pub(crate) use presentation::{
    PresentationFeedbackSource, PresentationFeedbackTarget, presentation_flags,
};
pub(crate) use seats::{SeatDevice, SeatSet};
pub(crate) use session_lock::UnlockSync;
pub(crate) use surfaces::NativeSurfaces;
pub(crate) use watch::WatchSources;

pub(crate) mod control;
pub(crate) mod device;
mod extensions;
pub(crate) mod focus_grab;
mod input;
mod key;
pub(crate) mod layer;
mod outputs;
mod owned_wl_surface;
pub(crate) mod preferences;
mod presentation;
mod seats;
pub(crate) mod session_lock;
pub(crate) mod startup;
mod surfaces;
pub(crate) mod watch;
pub(crate) mod window_input;
#[cfg(test)]
pub(crate) mod wire_messages;

pub(crate) enum NativeRole {
    Window { window: NativeWindow },
    Layer(ZwlrLayerSurfaceV1),
    Lock(session_lock::LockSurface),
}

impl Drop for NativeRole {
    fn drop(&mut self) {
        match self {
            Self::Layer(layer) if layer.is_alive() => layer.destroy(),
            Self::Lock(surface) if surface.is_alive() => surface.destroy(),
            _ => {}
        }
    }
}

pub(crate) struct Surface {
    accessibility: Option<Bridge>,
    extensions: SurfaceExtensions,
    role: NativeRole,
    window: Arc<OwnedWlSurface>,
}

impl Surface {
    pub(crate) fn new(
        window: Arc<OwnedWlSurface>,
        extensions: SurfaceExtensions,
        role: NativeRole,
        accessibility: Bridge,
    ) -> Self {
        Self {
            accessibility: Some(accessibility),
            extensions,
            role,
            window,
        }
    }

    pub(crate) fn wl(&self) -> &wl_surface::WlSurface {
        self.window.wl()
    }

    pub(crate) fn follows_fractional_scale(&self) -> bool {
        self.extensions.fractional.is_some()
    }
}

pub(crate) struct SurfaceProtocols {
    xdg: Option<XdgShell>,
    layers: Option<ZwlrLayerShellV1>,
    lock: session_lock::SessionLock,
    viewporter: Option<WpViewporter>,
    fractional: Option<WpFractionalScaleManagerV1>,
    background: Option<ext_background_effect_manager_v1::ExtBackgroundEffectManagerV1>,
}

pub(crate) fn configure_event(
    width: Option<NonZero<u32>>,
    height: Option<NonZero<u32>>,
    token: Option<ConfigureToken>,
    window: Option<WindowState>,
) -> SurfaceEvent {
    let length = |pixels: NonZero<u32>| Dp::new(f64::from(pixels.get()));
    SurfaceEvent::Configured {
        width: width.map(length),
        height: height.map(length),
        token,
        window,
    }
}
