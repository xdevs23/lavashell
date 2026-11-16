// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::units::NumberError;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("the content of a panel measures a length that is negative or not finite")]
    Content(#[source] NumberError),
}
