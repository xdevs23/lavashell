// SPDX-License-Identifier: GPL-3.0-only
use super::control::RoleControl;
use super::{NativeRole, SurfaceProtocols};
use crate::Error;
use crate::logical::whole_size;
use crate::translator::Translator;
use lavashell_platform::WindowRole;
use sctk::reexports::client::QueueHandle;
use sctk::reexports::client::globals::GlobalList;
use sctk::reexports::client::protocol::wl_surface;
use sctk::shell::WaylandSurface;
use sctk::shell::xdg::{XdgShell, window::WindowDecorations};
use smithay_client_toolkit as sctk;

impl SurfaceProtocols {
    pub(crate) fn bind(globals: &GlobalList, qh: &QueueHandle<Translator>) -> Self {
        Self {
            xdg: XdgShell::bind(globals, qh).ok(),
            layers: globals.bind(qh, 1..=5, ()).ok(),
            viewporter: globals.bind(qh, 1..=1, ()).ok(),
            fractional: globals.bind(qh, 1..=1, ()).ok(),
            background: globals.bind(qh, 1..=1, ()).ok(),
        }
    }
}

impl Translator {
    /// Gives `wl` the role of `window`. A window that draws the decorations itself asks the compositor to draw none.
    pub(super) fn window_role(
        &self,
        wl: &wl_surface::WlSurface,
        window: &WindowRole,
    ) -> Result<(NativeRole, RoleControl), Error> {
        let xdg = self.protocols.xdg.as_ref().ok_or(Error::NoXdgShell)?;
        let decorations = if window.draws_decorations {
            WindowDecorations::ClientOnly
        } else {
            WindowDecorations::RequestServer
        };
        let native = xdg.create_window(wl.clone(), decorations, &self.qh);
        native.set_title(&window.title);
        native.set_app_id(&window.app_id);
        let min_size = (window.min_size)
            .map(|minimum| whole_size(minimum).ok_or(Error::MinimumWindowSize(minimum)))
            .transpose()?;
        native.set_min_size(min_size);
        if window.fullscreen {
            native.set_fullscreen(None);
        }
        native.commit();
        Ok((
            NativeRole::Window { window: native },
            RoleControl::Stateless,
        ))
    }
}
