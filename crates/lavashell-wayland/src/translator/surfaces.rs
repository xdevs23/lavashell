// SPDX-License-Identifier: GPL-3.0-only
use super::Translator;
use crate::accessibility::Bridge;
use crate::logical::to_protocol_position;
use crate::native::{
    Control, LayerTarget, NativeRole, OwnedWlSurface, RoleControl, Surface, SurfaceExtensions,
    SurfaceObjects, initial_role_state, protocol_serial,
};
use crate::protocol_serial::ProtocolSerial;
use crate::sctk;
use crate::{Error, LayerError, WindowControlError};
use lavashell_core::surface::SurfaceId;
use lavashell_platform::{
    AccessibilityRequest, PanelRequest, PointerPress, Role, SurfaceAttachment, SurfaceEvent,
    SurfaceRequest, SurfaceSpec, WindowControlRequest,
};
use sctk::reexports::client::protocol::wl_seat::WlSeat;
use sctk::reexports::client::protocol::wl_surface;
use std::sync::Arc;

impl Translator {
    pub(super) fn find_surface(&self, wl: &wl_surface::WlSurface) -> Option<SurfaceId> {
        self.surfaces.find_live(|known| known.wl() == wl)
    }

    pub(super) fn live_wl_surface(&self, surface: SurfaceId) -> Option<wl_surface::WlSurface> {
        self.surfaces.live(surface).map(|known| known.wl().clone())
    }

    pub(super) fn execute_surface(&mut self, request: SurfaceRequest) -> Result<(), Error> {
        match request {
            SurfaceRequest::Create { surface, spec } => self.create_surface(surface, &spec),
            SurfaceRequest::Destroy(surface) => {
                self.surfaces.retire(surface);
                self.links.attach(SurfaceAttachment::Detach(surface));
                Ok(())
            }
            SurfaceRequest::Release(surface) => {
                drop(self.surfaces.release(surface));
                Ok(())
            }
        }
    }

    pub(super) fn execute_accessibility(&mut self, request: AccessibilityRequest) {
        match request {
            AccessibilityRequest::Publish { surface, tree } => {
                if let Some(bridge) = self.surfaces.bridge_mut(surface) {
                    bridge.publish(*tree);
                }
            }
            AccessibilityRequest::SetWindowFocus { surface, focused } => {
                if let Some(bridge) = self.surfaces.bridge_mut(surface) {
                    bridge.window_focus(focused);
                }
            }
        }
    }

    pub(super) fn execute_panel(&self, request: PanelRequest) -> Result<(), LayerError> {
        match request {
            PanelRequest::SetKeyboard(surface, keyboard) => (self.surfaces.live(surface))
                .map_or(Ok(()), |native| native.send_keyboard(surface, keyboard)),
        }
    }

    pub(super) fn execute_window_control(
        &self,
        request: WindowControlRequest,
    ) -> Result<(), Error> {
        match request {
            WindowControlRequest::BeginMove { surface, press } => {
                self.control_window(surface, |known| {
                    let (seat, serial) = self.press_seat(press)?;
                    Ok(known.begin_move(surface, seat, serial)?)
                })
            }
            WindowControlRequest::BeginResize {
                surface,
                press,
                edge,
            } => self.control_window(surface, |known| {
                let (seat, serial) = self.press_seat(press)?;
                Ok(known.begin_resize(surface, seat, serial, edge)?)
            }),
            WindowControlRequest::SetMaximized { surface, maximized } => {
                self.control_window(surface, |known| Ok(known.set_maximized(surface, maximized)?))
            }
            WindowControlRequest::Minimize { surface } => {
                self.control_window(surface, |known| Ok(known.minimize(surface)?))
            }
            WindowControlRequest::SetFullscreen {
                surface,
                fullscreen,
            } => self.control_window(surface, |known| {
                Ok(known.set_fullscreen(surface, fullscreen)?)
            }),
            WindowControlRequest::ShowMenu { surface, press, at } => {
                self.control_window(surface, |known| {
                    let Some(position) = to_protocol_position(at) else {
                        log::warn!(
                            "the window menu of {surface:?} does not open: {at:?} does not fit the 32-bit coordinates of the protocol"
                        );
                        return Ok(());
                    };
                    let (seat, serial) = self.press_seat(press)?;
                    Ok(known.show_window_menu(surface, seat, serial, position)?)
                })
            }
        }
    }

    fn press_seat(&self, press: PointerPress) -> Result<(&WlSeat, ProtocolSerial), Error> {
        let serial = protocol_serial(press.serial)?;
        let seat = (self.devices.pointer_seat(press.pointer))
            .ok_or(WindowControlError::NoSeat(press.pointer))?;
        Ok((seat, serial))
    }

    fn control_window(
        &self,
        surface: SurfaceId,
        control: impl FnOnce(&Surface) -> Result<(), Error>,
    ) -> Result<(), Error> {
        (self.surfaces.live(surface)).map_or(Ok(()), control)
    }

    fn create_surface(&mut self, surface: SurfaceId, spec: &SurfaceSpec) -> Result<(), Error> {
        let wl = self.compositor.create_surface(&self.qh);
        let window = Arc::new(OwnedWlSurface::new(wl.clone(), self.connection.backend()));
        let extensions = SurfaceExtensions::create(
            &self.protocols,
            &wl,
            &self.qh,
            surface,
            spec.appearance.background_blur,
        );
        let Some((role, role_control)) = self.create_role(&wl, surface, &spec.role)? else {
            self.links.report((surface, SurfaceEvent::Closed));
            return Ok(());
        };
        let control = self.surface_control(surface, &wl, &extensions, &spec.role, role_control);
        let attach = spec.attach(surface, window.clone(), control);
        let accessibility = Bridge::new(surface, self.view.clone());
        let native = Surface::new(window, extensions, role, accessibility);
        self.surfaces.insert(surface, native);
        self.links
            .attach(SurfaceAttachment::Attach(Box::new(attach)));
        Ok(())
    }

    fn surface_control(
        &self,
        surface: SurfaceId,
        wl: &wl_surface::WlSurface,
        extensions: &SurfaceExtensions,
        role: &Role,
        role_control: RoleControl,
    ) -> Control {
        let presentation = (self.presentation.as_ref()).map(|source| source.proxy().clone());
        let objects = SurfaceObjects::new(
            self.connection.clone(),
            wl.clone(),
            extensions,
            presentation,
            self.compositor.wl_compositor().clone(),
            role_control,
        );
        Control::new(surface, objects, initial_role_state(role), self.qh.clone())
    }

    fn create_role(
        &self,
        wl: &wl_surface::WlSurface,
        surface: SurfaceId,
        role: &Role,
    ) -> Result<Option<(NativeRole, RoleControl)>, Error> {
        Ok(Some(match role {
            Role::Window(window) => self.protocols.window_role(wl, window, &self.qh)?,
            Role::Panel(panel) => {
                let output = match panel.output.map(|output| self.find_output(output)) {
                    Some(None) => return Ok(None),
                    chosen => chosen.flatten(),
                };
                let target = LayerTarget {
                    surface,
                    wl,
                    output: output.as_ref(),
                };
                self.protocols.panel_role(&target, panel, &self.qh)?
            }
            Role::Lock { output } => {
                let Some(output) = self.find_output(*output) else {
                    return Ok(None);
                };
                self.protocols.lock_role(wl, &output, &self.qh, surface)?
            }
        }))
    }
}
