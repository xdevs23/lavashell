// SPDX-License-Identifier: GPL-3.0-only
use super::device::protocol_serial;
use super::{Platform, Role};
use crate::Error;
use lavashell_platform::WindowControlRequest;

impl Platform {
    /// Executes `request` while the window and the pointer in it exist, and returns whether a move of the window
    /// started.
    pub(super) fn execute_window_control(
        &self,
        request: WindowControlRequest,
    ) -> Result<bool, Error> {
        match request {
            WindowControlRequest::BeginMove {
                surface,
                pointer,
                serial,
            } => {
                let serial = protocol_serial(serial)?;
                let role = (self.surfaces.get(&surface)).map(|known| &known.role);
                let started = match (self.pointer_seat(pointer), role) {
                    (Some(seat), Some(Role::Window { window })) => {
                        window.move_(seat, serial);
                        true
                    }
                    _ => false,
                };
                Ok(started)
            }
        }
    }
}
