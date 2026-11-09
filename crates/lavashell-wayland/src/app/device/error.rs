// SPDX-License-Identifier: GPL-3.0-only
use std::num::TryFromIntError;

/// A failure of an input device. Each variant is one operation that can fail.
#[derive(Debug, thiserror::Error)]
pub enum DeviceError {
    #[error("the serial of an input event does not fit a Wayland serial")]
    ProtocolSerial(#[source] TryFromIntError),
}
