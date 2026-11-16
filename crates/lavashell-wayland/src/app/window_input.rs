// SPDX-License-Identifier: GPL-3.0-only
use super::{NativeRole, Surface};
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
        let NativeRole::Window { window } = &self.role else {
            return Err(WindowControlError::NotAWindow(id));
        };
        window.move_(seat, serial.get());
        Ok(())
    }
}
