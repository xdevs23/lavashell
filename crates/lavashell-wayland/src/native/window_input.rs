// SPDX-License-Identifier: GPL-3.0-only
use super::{NativeRole, NativeWindow, Surface};
use crate::protocol_serial::ProtocolSerial;
use crate::sctk;
use lavashell_core::surface::SurfaceId;
use sctk::reexports::client::protocol::wl_seat::WlSeat;

mod error;

pub use error::WindowControlError;

impl Surface {
    pub(crate) fn begin_move(
        &self,
        id: SurfaceId,
        seat: &WlSeat,
        serial: ProtocolSerial,
    ) -> Result<(), WindowControlError> {
        self.window(id)?.move_(seat, serial.get());
        Ok(())
    }

    pub(crate) fn set_maximized(
        &self,
        id: SurfaceId,
        maximized: bool,
    ) -> Result<(), WindowControlError> {
        let window = self.window(id)?;
        if maximized {
            window.set_maximized();
        } else {
            window.unset_maximized();
        }
        Ok(())
    }

    pub(crate) fn minimize(&self, id: SurfaceId) -> Result<(), WindowControlError> {
        self.window(id)?.set_minimized();
        Ok(())
    }

    pub(crate) fn set_fullscreen(
        &self,
        id: SurfaceId,
        fullscreen: bool,
    ) -> Result<(), WindowControlError> {
        let window = self.window(id)?;
        if fullscreen {
            window.set_fullscreen(None);
        } else {
            window.unset_fullscreen();
        }
        Ok(())
    }

    pub(crate) fn show_window_menu(
        &self,
        id: SurfaceId,
        seat: &WlSeat,
        serial: ProtocolSerial,
        position: (i32, i32),
    ) -> Result<(), WindowControlError> {
        self.window(id)?
            .show_window_menu(seat, serial.get(), position);
        Ok(())
    }

    fn window(&self, id: SurfaceId) -> Result<&NativeWindow, WindowControlError> {
        let NativeRole::Window { window } = &self.role else {
            return Err(WindowControlError::NotAWindow(id));
        };
        Ok(window)
    }
}
