// SPDX-License-Identifier: GPL-3.0-only
use calloop::InsertError;
use calloop::futures::ExecutorDestroyed;
use lavashell_event_loop::EventLoopFailure;
use lavashell_test_fixtures::test_bus::TestBusError;

#[derive(Debug, thiserror::Error)]
pub(crate) enum TestError {
    #[error("the bus failed")]
    Bus(#[from] zbus::Error),
    #[error("the event loop failed")]
    EventLoop(#[source] EventLoopFailure),
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
        Self::EventLoop(failure.error.into())
    }
}

impl From<calloop::Error> for TestError {
    fn from(error: calloop::Error) -> Self {
        Self::EventLoop(error.into())
    }
}
