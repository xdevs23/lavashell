// SPDX-License-Identifier: GPL-3.0-only
use calloop::InsertError;
use calloop::futures::ExecutorDestroyed;
use lavashell_test_fixtures::test_bus::TestBusError;

#[derive(Debug, thiserror::Error)]
pub(crate) enum TestError {
    #[error("the bus failed")]
    Bus(#[from] zbus::Error),
    #[error("the event loop failed")]
    EventLoop(#[from] calloop::Error),
    #[error(transparent)]
    TestBus(#[from] TestBusError),
    #[error("the watch on the appearance could not be started")]
    ScheduleWatch(#[source] ExecutorDestroyed),
    #[error("the watch on the portal stopped sending updates before the test ended")]
    UpdatesClosed,
    #[error("the watch on the portal ended before the test did")]
    WatchEnded,
}

impl<T> From<InsertError<T>> for TestError {
    fn from(failure: InsertError<T>) -> Self {
        Self::EventLoop(failure.error)
    }
}
