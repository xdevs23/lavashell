// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::test_error::TestError;
use lavashell_actor::mailbox;
use lavashell_core::surface::SurfaceId;
use lavashell_platform::SurfaceRequest;
use sctk::reexports::calloop::EventLoop;
use sctk::reexports::calloop::ping::{PingSource, make_ping};
use std::os::unix::net::UnixStream;
use std::time::Duration;

const FIRST: SurfaceId = SurfaceId(1);
const SECOND: SurfaceId = SurfaceId(2);

fn release(surface: SurfaceId) -> PlatformRequest {
    PlatformRequest::Surface(SurfaceRequest::Release(surface))
}

/// Takes the requests that arrived in `requests`, and returns the surface of each released one in the order they ran
/// and how serving ends. The release of `failing` fails.
fn take(
    requests: &Receiver<PlatformRequest>,
    wake: &Ping,
    failing: Option<SurfaceId>,
) -> (Vec<SurfaceId>, Option<Result<(), Error>>) {
    let mut released = Vec::new();
    let end = execute_arrived(requests, wake, |request| {
        let PlatformRequest::Surface(SurfaceRequest::Release(surface)) = request else {
            return Ok(());
        };
        if failing == Some(surface) {
            return Err(Error::NoXdgShell);
        }
        released.push(surface);
        Ok(())
    });
    (released, end)
}

/// A loop that counts the wakes of `arrivals` into its data.
fn counting_loop(
    arrivals: PingSource,
) -> Result<(EventLoop<'static, usize>, RegistrationToken), TestError> {
    let event_loop = EventLoop::try_new()?;
    let source =
        (event_loop.handle()).insert_source(arrivals, |(), (), wakes: &mut usize| *wakes += 1)?;
    Ok((event_loop, source))
}

#[test]
fn the_requests_run_in_order_and_serving_ends_once_every_sender_is_gone() -> Result<(), TestError> {
    let (sender, receiver) = mailbox();
    let (wake, _arrivals) = make_ping().map_err(Error::CreateRequestWake)?;
    sender.push(release(FIRST))?;
    sender.push(release(SECOND))?;
    drop(sender);
    let (released, end) = take(&receiver, &wake, None);
    assert_eq!(released, [FIRST, SECOND]);
    assert!(end.is_none());
    let (released, end) = take(&receiver, &wake, None);
    assert!(released.is_empty());
    assert!(matches!(end, Some(Ok(()))));
    Ok(())
}

#[test]
fn a_failed_request_ends_serving_before_the_requests_after_it() -> Result<(), TestError> {
    let (sender, receiver) = mailbox();
    let (wake, _arrivals) = make_ping().map_err(Error::CreateRequestWake)?;
    sender.push(release(FIRST))?;
    sender.push(release(SECOND))?;
    let (released, end) = take(&receiver, &wake, Some(FIRST));
    assert!(released.is_empty());
    assert!(matches!(end, Some(Err(Error::NoXdgShell))));
    Ok(())
}

#[test]
fn a_take_that_found_requests_wakes_the_next_take() -> Result<(), TestError> {
    let (wake, arrivals) = make_ping().map_err(Error::CreateRequestWake)?;
    let (mut event_loop, _source) = counting_loop(arrivals)?;
    let (sender, receiver) = mailbox();
    sender.push(release(FIRST))?;
    let mut wakes = 0;
    let _found = take(&receiver, &wake, None);
    event_loop.dispatch(Duration::ZERO, &mut wakes)?;
    assert_eq!(wakes, 1);
    let _found_nothing = take(&receiver, &wake, None);
    event_loop.dispatch(Duration::ZERO, &mut wakes)?;
    assert_eq!(wakes, 1);
    Ok(())
}

#[test]
fn a_closed_intake_takes_no_request_until_it_opens() -> Result<(), TestError> {
    let (wake, arrivals) = make_ping().map_err(Error::CreateRequestWake)?;
    let (mut event_loop, source) = counting_loop(arrivals)?;
    let handle = event_loop.handle();
    let (_sender, receiver) = mailbox::<PlatformRequest>();
    let mut requests = Requests::new(receiver.intake(), source);
    let (socket, _peer) = UnixStream::pair().map_err(TestError::CreateSocketPair)?;
    requests.close(&handle, socket.as_fd())?;
    requests.close(&handle, socket.as_fd())?;
    wake.ping();
    let mut wakes = 0;
    event_loop.dispatch(Duration::ZERO, &mut wakes)?;
    assert_eq!(wakes, 0);
    requests.open(&handle)?;
    event_loop.dispatch(Duration::ZERO, &mut wakes)?;
    assert_eq!(wakes, 1);
    Ok(())
}

#[test]
fn the_first_failure_ends_serving_whatever_is_recorded_after_it() {
    let mut ending = Ending::default();
    assert!(ending.take().is_none());
    ending.record(Ok(()));
    ending.record(Err(Error::NoXdgShell));
    ending.record(Err(Error::NoLayerShell));
    ending.record(Ok(()));
    assert!(matches!(ending.take(), Some(Err(Error::NoXdgShell))));
    assert!(ending.take().is_none());
}

#[test]
fn an_end_without_a_failure_is_recorded_until_it_is_taken() {
    let mut ending = Ending::default();
    ending.record(Ok(()));
    assert!(matches!(ending.take(), Some(Ok(()))));
    assert!(ending.take().is_none());
}
