// SPDX-License-Identifier: GPL-3.0-only
use super::NativeRole;
use super::device::protocol_serial;
use crate::Error;
use crate::translator::Translator;
use lavashell_platform::WindowControlRequest;

impl Translator {
    /// Executes `request` while the window and the pointer in it exist.
    pub(crate) fn execute_window_control(
        &self,
        request: WindowControlRequest,
    ) -> Result<(), Error> {
        match request {
            WindowControlRequest::BeginMove {
                surface,
                pointer,
                serial,
            } => {
                let serial = protocol_serial(serial)?;
                let seat = self.pointer_seat(pointer);
                let role = self.surfaces.live(surface).map(|known| &known.role);
                if let (Some(seat), Some(NativeRole::Window { window })) = (seat, role) {
                    window.move_(seat, serial);
                }
                Ok(())
            }
        }
    }
}
