// SPDX-License-Identifier: GPL-3.0-only
#[derive(Debug, thiserror::Error)]
pub(crate) enum PreferencesTestError {
    #[error("the portal watch closed its updates before the test ended")]
    UpdatesClosed,
    #[error("the portal watch ended before the test did")]
    WatchEnded,
    #[error("the portal watch failed before the test ended")]
    WatchFailed(#[source] zbus::Error),
}
