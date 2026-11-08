// SPDX-License-Identifier: GPL-3.0-only
//! The `hyprland-focus-grab-v1` protocol. A compositor that supports it limits input focus to the grabbed layer, such
//! as a popover or a panel, until the user interacts outside it.

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
