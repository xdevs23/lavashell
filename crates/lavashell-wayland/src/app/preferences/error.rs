// SPDX-License-Identifier: GPL-3.0-only
use sctk::reexports::calloop;
use sctk::reexports::calloop::futures::ExecutorDestroyed;
use smithay_client_toolkit as sctk;

/// A failure while watching the system preferences. Each variant is one operation that can fail.
#[derive(Debug, thiserror::Error)]
pub enum PreferencesError {
    #[error("the system preferences could not join the event loop")]
    WatchPreferences(#[source] calloop::Error),
    #[error("the watch of the system preferences could not be created")]
    CreatePreferenceWatch(#[source] calloop::Error),
    #[error("the watch of the system preferences could not be started")]
    SchedulePreferenceWatch(#[source] ExecutorDestroyed),
}
