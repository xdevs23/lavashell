// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::accessibility::ActionError;
use lavashell_core::units::NumberError;

/// A failure of the runtime, named after the operation that failed.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("every surface id is used")]
    NumberSurface,
    #[error("a new watch has a number too large for a watch id")]
    NumberWatch,
    #[error("the content of a surface could not be measured")]
    MeasureView(#[source] NumberError),
}

/// A failure while the runtime handles an event of an assistive technology. `E` is the failure of the platform to
/// show a frame.
#[derive(Debug, thiserror::Error)]
pub enum AccessibilityFailure<E> {
    #[error("a surface could not show the frame that an assistive technology reads")]
    Show(#[source] E),
    #[error("the app could not perform an action requested by an assistive technology")]
    PerformAction(#[source] ActionError),
}
