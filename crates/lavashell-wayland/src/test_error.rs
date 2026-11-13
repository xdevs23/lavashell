// SPDX-License-Identifier: GPL-3.0-only
//! The failures that the tests of this crate return in place of a panic.

use crate::ControlError;
use crate::Error;
use crate::LayerLayoutError;
use lavashell_core::units::NumberError;
use smithay_client_toolkit::reexports::calloop::{self, InsertError};
use std::env::VarError;
use std::num::TryFromIntError;
use std::{io, sync::mpsc};

#[derive(Debug, thiserror::Error)]
pub(crate) enum TestError {
    #[error("the host failed")]
    Host(#[from] Error),
    #[error("the renderer failed")]
    Renderer(#[from] lavashell_renderer::Error),
    #[error("the event loop failed")]
    EventLoop(#[from] calloop::Error),
    #[error("the other end of a link is gone")]
    LinkClosed(#[from] lavashell_actor::Closed),
    #[error("the bus failed")]
    Bus(#[from] zbus::Error),
    #[error("a property of the bus could not be set")]
    SetProperty(#[from] zbus::fdo::Error),
    #[error("the object path is not valid")]
    ObjectPath(#[from] zbus::zvariant::Error),
    #[error("the address of the session bus is not set")]
    BusAddress(#[from] VarError),
    #[error("the path of the test executable is unknown")]
    CurrentExecutable(#[source] io::Error),
    #[error("the fixture received no message")]
    Receive(#[from] mpsc::RecvError),
    #[error("a number could not be converted")]
    Number(#[from] NumberError),
    #[error("a number does not fit its type")]
    Narrowing(#[from] TryFromIntError),
    #[error("the socket pair could not be created")]
    CreateSocketPair(#[source] io::Error),
    #[error("a request for a native surface does not fit the protocol")]
    Control(#[from] ControlError),
    #[error("the layout of a layer does not fit the protocol")]
    LayerLayout(#[from] LayerLayoutError),
    #[error("the pipe could not be created")]
    CreatePipe(#[source] rustix::io::Errno),
    #[error(transparent)]
    AccessibilityTest(#[from] crate::accessibility::test_error::AccessibilityTestError),
    #[error(transparent)]
    LayerTest(#[from] crate::app::layer::test_error::LayerTestError),
    #[error(transparent)]
    PipesTest(#[from] crate::clipboard::pipes::test_error::PipesTestError),
    #[error(transparent)]
    PreferencesTest(#[from] crate::preferences::test_error::PreferencesTestError),
    #[error(transparent)]
    TestBus(#[from] crate::test_bus::test_error::TestBusError),
    #[error(transparent)]
    WireTest(#[from] crate::app::wire_messages::test_error::WireTestError),
}

impl<T> From<InsertError<T>> for TestError {
    fn from(failure: InsertError<T>) -> Self {
        Self::EventLoop(failure.error)
    }
}
