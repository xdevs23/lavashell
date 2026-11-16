// SPDX-License-Identifier: GPL-3.0-only
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProtocolSerial(u32);

impl ProtocolSerial {
    pub(crate) fn new(serial: u32) -> Self {
        Self(serial)
    }

    pub(crate) fn get(self) -> u32 {
        self.0
    }
}
