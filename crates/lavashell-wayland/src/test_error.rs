// SPDX-License-Identifier: GPL-3.0-only
//! The failures that the tests of this crate return in place of a panic.

use crate::{ControlError, Error, LayerLayoutError};
use lavashell_core::scene::tree::ApplyError;
use lavashell_core::units::NumberError;
use lavashell_test_fixtures::FrameFailure;
use log::SetLoggerError;
use smithay_client_toolkit::reexports::calloop::{self, InsertError};
use std::array::TryFromSliceError;
use std::env::VarError;
use std::num::TryFromIntError;
use std::process::ExitStatus;
use std::{io, sync::mpsc};

#[derive(Debug, thiserror::Error)]
pub(crate) enum TestError {
    #[error("the host failed")]
    Host(#[from] Error),
    #[error("the renderer failed")]
    Renderer(#[from] lavashell_renderer::Error),
    #[error("the event loop failed")]
    EventLoop(#[from] calloop::Error),
    #[error("the bus failed")]
    Bus(#[from] zbus::Error),
    #[error("a property of the bus could not be set")]
    SetProperty(#[from] zbus::fdo::Error),
    #[error("the object path is not valid")]
    ObjectPath(#[from] zbus::zvariant::Error),
    #[error("the address of the session bus is not set")]
    BusAddress(#[from] VarError),
    #[error("the dbus-daemon could not start")]
    StartBusDaemon(#[source] io::Error),
    #[error("the address of the private bus could not be read")]
    ReadBusAddress(#[source] io::Error),
    #[error("the dbus-daemon printed no address")]
    NoBusAddress,
    #[error("the path of the test executable is unknown")]
    CurrentExecutable(#[source] io::Error),
    #[error("the test executable could not be opened")]
    OpenExecutable(#[source] io::Error),
    #[error("the child test could not run")]
    RunChildTest(#[source] io::Error),
    #[error("the child test ended with {0}")]
    ChildTestFailed(ExitStatus),
    #[error("the bus closed before the change arrived")]
    BusClosed,
    #[error("the connection of the bus has no unique name")]
    NoUniqueName,
    #[error("the fixture received no message")]
    Receive(#[from] mpsc::RecvError),
    #[error("the bridge closed its requests before the awaited one arrived")]
    RequestsClosed,
    #[error("the bridge sent another event than the awaited one")]
    UnexpectedEvent,
    #[error("the portal watch closed its updates before the test ended")]
    UpdatesClosed,
    #[error("the portal watch ended before the test did")]
    WatchEnded,
    #[error("the portal watch failed before the test ended")]
    WatchFailed(#[source] zbus::Error),
    #[error("a number could not be converted")]
    Number(#[from] NumberError),
    #[error("a number does not fit its type")]
    Narrowing(#[from] TryFromIntError),
    #[error("a protocol word is shorter than four bytes")]
    ShortWord(#[from] TryFromSliceError),
    #[error("the requests of the client could not be read")]
    ReadProtocol(#[source] io::Error),
    #[error("the socket pair could not be created")]
    CreateSocketPair(#[source] io::Error),
    #[error("the socket could not be made non-blocking")]
    ConfigureSocket(#[source] io::Error),
    #[error("the wire has no layer")]
    NoLayer,
    #[error("a request for a native surface does not fit the protocol")]
    Control(#[from] ControlError),
    #[error("the layout of a layer does not fit the protocol")]
    LayerLayout(#[from] LayerLayoutError),
    #[error("the pipe could not be created")]
    CreatePipe(#[source] rustix::io::Errno),
    #[error("the file is not a pipe")]
    NotPipe,
    #[error(transparent)]
    Failed(#[from] FrameFailure),
    #[error("the scene could not be built")]
    Scene(#[from] ApplyError),
    #[error("the test process has a logger that is not the recorder of the test")]
    Logger(#[source] &'static SetLoggerError),
}

impl<T> From<InsertError<T>> for TestError {
    fn from(failure: InsertError<T>) -> Self {
        Self::EventLoop(failure.error)
    }
}
