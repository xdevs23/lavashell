// SPDX-License-Identifier: GPL-3.0-only
#[derive(Debug, thiserror::Error)]
pub(crate) enum AccessibilityTestError {
    #[error("the bus closed before the change arrived")]
    BusClosed,
    #[error("the connection of the bus has no unique name")]
    NoUniqueName,
    #[error("the bridge stopped reporting events before the awaited one arrived")]
    EventsClosed,
    #[error("the bridge sent another event than the awaited one")]
    UnexpectedEvent,
}
