// SPDX-License-Identifier: GPL-3.0-only
use super::control::RoleControl;
use super::session_lock::SessionLock;
use super::{NativeRole, SurfaceProtocols};
use crate::bind::{bind_global, bind_optional};
use crate::logical::to_protocol_size;
use crate::sctk;
use crate::translator::Translator;
use lavashell_platform::{Capability, WindowRole};
use sctk::reexports::client::globals::GlobalList;
use sctk::reexports::client::protocol::wl_surface;
use sctk::reexports::client::{Proxy, QueueHandle};
use sctk::reexports::protocols::xdg::shell::client::xdg_wm_base::XdgWmBase;
use sctk::shell::WaylandSurface;
use sctk::shell::xdg::{XdgShell, window::WindowDecorations};

mod error;

pub use error::WindowError;

impl SurfaceProtocols {
    pub(crate) fn bind(globals: &GlobalList, qh: &QueueHandle<Translator>) -> Self {
        Self {
            xdg: bind_optional(XdgWmBase::interface().name, XdgShell::bind(globals, qh)),
            layers: bind_global(globals, qh),
            lock: SessionLock::bind(globals, qh),
            viewporter: bind_global(globals, qh),
            fractional: bind_global(globals, qh),
            background: bind_global(globals, qh),
        }
    }

    pub(crate) fn capabilities(&self) -> impl Iterator<Item = Capability> {
        [
            self.layers.is_some().then_some(Capability::Panels),
            self.lock.capability(),
        ]
        .into_iter()
        .flatten()
    }

    pub(crate) fn window_role(
        &self,
        wl: &wl_surface::WlSurface,
        window: &WindowRole,
        qh: &QueueHandle<Translator>,
    ) -> Result<(NativeRole, RoleControl), WindowError> {
        let xdg = self.xdg.as_ref().ok_or(WindowError::NoXdgShell)?;
        let decorations = if window.draws_decorations {
            WindowDecorations::ClientOnly
        } else {
            WindowDecorations::RequestServer
        };
        let native = xdg.create_window(wl.clone(), decorations, qh);
        native.set_title(&window.title);
        native.set_app_id(&window.app_id);
        let min_size = (window.min_size)
            .map(|minimum| {
                to_protocol_size(minimum).ok_or(WindowError::MinimumSizeBeyondProtocol(minimum))
            })
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
