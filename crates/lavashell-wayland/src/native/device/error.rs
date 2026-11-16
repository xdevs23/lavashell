// SPDX-License-Identifier: GPL-3.0-only
use std::num::TryFromIntError;

#[derive(Debug, thiserror::Error)]
pub enum DeviceError {
    #[error("the serial of an input event does not fit a Wayland serial")]
    ProtocolSerial(#[source] TryFromIntError),
}
