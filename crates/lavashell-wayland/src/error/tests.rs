// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::sctk::reexports::calloop;
use crate::sctk::reexports::client::ConnectError;
use crate::sctk::reexports::client::backend::WaylandError;
use crate::sctk::reexports::client::backend::protocol::ProtocolError;
use crate::sctk::reexports::client::globals::{BindError, GlobalError};
use crate::{
    ConnectionError, ControlError, DragSourceError, EventLoopError, FocusGrabError, LayerError,
    RequestsError, SessionLockError, WatchError, WindowControlError, WindowError,
};
use lavashell_core::surface::SurfaceId;
use lavashell_event_loop::EventLoopFailure;
use lavashell_platform::{DragSourceId, PointerDevice};
use std::io;

const SURFACE: SurfaceId = SurfaceId::numbered(0);

fn too_many_open_files() -> io::Error {
    io::Error::from_raw_os_error(24)
}

fn connection_closed_by_the_compositor() -> io::Error {
    io::Error::from(io::ErrorKind::BrokenPipe)
}

fn event_loop_out_of_file_descriptors() -> EventLoopFailure {
    calloop::Error::IoError(too_many_open_files()).into()
}

fn assert_each_names(failures: Vec<Error>, step: NextStep) {
    for failure in failures {
        assert_eq!(failure.next_step(), step, "{failure}");
    }
}

#[test]
fn a_failure_without_a_working_wayland_session_asks_for_a_desktop_session() {
    assert_each_names(
        vec![
            ConnectionError::Connect(ConnectError::NoCompositor).into(),
            ConnectionError::ReadGlobals(GlobalError::Backend(WaylandError::Io(
                connection_closed_by_the_compositor(),
            )))
            .into(),
            ConnectionError::Flush(WaylandError::Io(connection_closed_by_the_compositor())).into(),
            EventLoopError::DispatchEvents(
                calloop::Error::IoError(connection_closed_by_the_compositor()).into(),
            )
            .into(),
        ],
        NextStep::StartInDesktopSession,
    );
}

#[test]
fn running_out_of_event_loop_watches_or_file_descriptors_asks_to_free_system_resources() {
    assert_each_names(
        vec![
            EventLoopError::CreateEventLoop(event_loop_out_of_file_descriptors()).into(),
            EventLoopError::WatchConnection(event_loop_out_of_file_descriptors()).into(),
            RequestsError::CreateRequestWake(too_many_open_files()).into(),
            RequestsError::WatchRequests(event_loop_out_of_file_descriptors()).into(),
            RequestsError::ResumeRequests(event_loop_out_of_file_descriptors()).into(),
            RequestsError::DuplicateSocket(too_many_open_files()).into(),
            RequestsError::WatchWritable(event_loop_out_of_file_descriptors()).into(),
            WatchError::WatchFd(event_loop_out_of_file_descriptors()).into(),
            WatchError::ResumeWatch(event_loop_out_of_file_descriptors()).into(),
        ],
        NextStep::FreeSystemResources,
    );
}

#[test]
fn a_compositor_without_a_protocol_the_app_uses_asks_for_a_supporting_platform() {
    assert_each_names(
        vec![
            ConnectionError::BindShm(BindError::NotPresent).into(),
            ConnectionError::BindCompositor(BindError::UnsupportedVersion).into(),
            FocusGrabError::NoFocusGrab.into(),
            LayerError::NoLayerShell.into(),
            SessionLockError::NotSupported.into(),
            WindowError::NoXdgShell.into(),
        ],
        NextStep::UseSupportingPlatform,
    );
}

#[test]
fn a_defect_in_the_program_asks_to_report_the_failure() {
    assert_each_names(
        vec![
            DragSourceError::DuplicateDrag(DragSourceId(1)).into(),
            FocusGrabError::WithoutSurface(SURFACE).into(),
            LayerError::PanelWithoutLayer(SURFACE).into(),
            RequestsError::PauseRequests(calloop::Error::InvalidToken.into()).into(),
            SessionLockError::AlreadyUsed.into(),
            SessionLockError::LockSurfaceWithoutLock(SURFACE).into(),
            SessionLockError::UnlockWithoutLock.into(),
            WindowControlError::NoSeat(PointerDevice(1)).into(),
            WindowControlError::NotAWindow(SURFACE).into(),
        ],
        NextStep::ReportTheFailure,
    );
}

#[test]
fn a_protocol_error_sent_by_the_compositor_asks_to_report_the_failure() {
    let protocol = || {
        WaylandError::Protocol(ProtocolError {
            code: 0,
            object_id: 1,
            object_interface: "wl_surface".to_owned(),
            message: "invalid request".to_owned(),
        })
    };
    assert_each_names(
        vec![
            ConnectionError::ReadGlobals(GlobalError::Backend(protocol())).into(),
            ConnectionError::Flush(protocol()).into(),
        ],
        NextStep::ReportTheFailure,
    );
    assert_eq!(
        ControlError::Flush(protocol()).next_step(),
        NextStep::ReportTheFailure
    );
    assert_eq!(
        ControlError::Flush(WaylandError::Io(connection_closed_by_the_compositor())).next_step(),
        NextStep::StartInDesktopSession
    );
}
