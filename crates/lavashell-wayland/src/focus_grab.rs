// SPDX-License-Identifier: GPL-3.0-only
//! Focus grabs for layer surfaces through the `hyprland-focus-grab-v1` protocol. A compositor that supports it
//! limits input focus to the grabbed layer, such as a popover or a panel, until the user interacts outside it.

use std::rc::Rc;

/// The focus grab of a layer surface. The layer grabs focus while `active` returns true. The compositor clears the
/// grab when the user interacts outside the layer or when another focus grab starts, and then `cleared` runs. A
/// new grab starts only after `active` returned false once. On a compositor without `hyprland-focus-grab-v1`,
/// the layer gets focus as layer-shell defines it.
#[derive(Clone)]
pub struct FocusGrab {
    pub(crate) active: Rc<dyn Fn() -> bool>,
    pub(crate) cleared: Rc<dyn Fn()>,
}

impl FocusGrab {
    pub fn new(active: impl Fn() -> bool + 'static, cleared: impl Fn() + 'static) -> Self {
        Self {
            active: Rc::new(active),
            cleared: Rc::new(cleared),
        }
    }
}

pub(crate) mod protocol {
    use crate::sctk::reexports::client as wayland_client;
    use wayland_client::protocol::wl_surface;

    pub mod __interfaces {
        use crate::sctk::reexports::client::protocol::__interfaces::{
            WL_SURFACE_INTERFACE, wl_surface_interface,
        };

        include!(concat!(env!("OUT_DIR"), "/focus_grab_interfaces.rs"));
    }

    use self::__interfaces::{
        HYPRLAND_FOCUS_GRAB_MANAGER_V1_INTERFACE, HYPRLAND_FOCUS_GRAB_V1_INTERFACE,
    };

    include!(concat!(env!("OUT_DIR"), "/focus_grab_client.rs"));
}
