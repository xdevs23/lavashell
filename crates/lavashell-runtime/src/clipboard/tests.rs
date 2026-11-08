// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use lavashell_core::geometry::Size;
use lavashell_core::preferences::PreferenceOverrides;
use lavashell_core::transfer::TransferError;
use lavashell_core::units::{Density, DpExt};
use lavashell_core::view::View;
use lavashell_platform::{SurfaceEvent, WatchEvent, WatchRequest};
use lavashell_test_fixtures::FrameFailure;
use std::cell::Cell;
use std::collections::BTreeSet;
use std::io::{Read, Write};

#[derive(Debug, thiserror::Error)]
pub(super) enum TestError {
    #[error("the runtime failed")]
    Runtime(#[from] Error),
    #[error("the transfer data is not valid")]
    Transfer(#[from] TransferError),
    #[error("a frame failed")]
    Frame(#[from] FrameFailure),
    #[error("a pipe failed")]
    Pipe(#[from] std::io::Error),
    #[error("{0} is missing")]
    Missing(&'static str),
}

pub(super) type TestResult = Result<(), TestError>;

type Outcome<T> = Rc<RefCell<Option<Result<T, Error>>>>;

struct Run {
    watches: Watches,
    transfers: ClipboardTransfers,
    records: SurfaceRecords,
    surface: SurfaceId,
}

impl Run {
    fn new() -> Result<Self, TestError> {
        let watches = Watches::default();
        let transfers = ClipboardTransfers::new(&watches);
        let mut records = SurfaceRecords::new(PreferenceOverrides::default());
        let view = View::new(lavashell_test_fixtures::facts(), |_| {});
        let surface = records.open(
            view,
            Size::new(100.dp(), 50.dp()),
            Density::ONE,
            None,
            String::new(),
        )?;
        Ok(Self {
            watches,
            transfers,
            records,
            surface,
        })
    }

    fn read(&self, formats: &[&str]) -> Outcome<ReceivedData> {
        let outcome = Rc::new(RefCell::new(None));
        let reported = outcome.clone();
        let clipboard = self.transfers.clipboard();
        clipboard.read(self.surface, formats.iter().copied(), move |result| {
            *reported.borrow_mut() = Some(result);
        });
        self.transfers.submit(&self.records);
        outcome
    }
}

/// Reports each watched pipe as ready until every watch ended.
pub(crate) fn move_bytes(watches: &Watches) {
    let mut open = BTreeSet::new();
    loop {
        for request in watches.take_requests() {
            let _changed = match request {
                WatchRequest::Watch(id, ..) => open.insert(id),
                WatchRequest::Resume(_) => false,
                WatchRequest::Unwatch(id) => open.remove(&id),
            };
        }
        if open.is_empty() {
            return;
        }
        for id in &open {
            watches.handle(WatchEvent::Ready(*id));
        }
    }
}

fn offer(formats: &[&str]) -> ClipboardEvent {
    let formats = formats.iter().map(|format| FormatId((*format).into()));
    ClipboardEvent::SelectionOffered(formats.collect())
}

fn set_selection(transfers: &ClipboardTransfers, data: TransferData) -> Outcome<()> {
    let outcome = Rc::new(RefCell::new(None));
    let reported = outcome.clone();
    let clipboard = transfers.clipboard();
    clipboard.set_data(data, move |result| {
        *reported.borrow_mut() = Some(result);
    });
    outcome
}

pub(super) fn set_request(
    requests: Vec<ClipboardRequest>,
) -> Result<(SelectionId, Vec<FormatId>), TestError> {
    (requests.into_iter())
        .find_map(|request| match request {
            ClipboardRequest::SetSelection(id, formats) => Some((id, formats)),
            ClipboardRequest::Receive(..) => None,
        })
        .ok_or(TestError::Missing("a selection"))
}

pub(super) fn receive_request(
    requests: Vec<ClipboardRequest>,
) -> Result<(ReceiveId, FormatId), TestError> {
    (requests.into_iter())
        .find_map(|request| match request {
            ClipboardRequest::Receive(id, format) => Some((id, format)),
            ClipboardRequest::SetSelection(..) => None,
        })
        .ok_or(TestError::Missing("a receive"))
}

#[test]
fn a_set_selection_is_written_to_each_client_that_pastes_it() -> TestResult {
    let run = Run::new()?;
    let data = TransferData::single("application/x-custom", vec![0, 255, 1])?;
    let set = set_selection(&run.transfers, data);
    run.transfers.submit(&run.records);
    let (id, formats) = set_request(run.transfers.take_requests())?;
    assert_eq!(formats, [FormatId("application/x-custom".into())]);
    assert!(set.borrow().is_none());
    run.transfers
        .handle(ClipboardEvent::SelectionSet(id, Ok(())));
    assert!(matches!(*set.borrow(), Some(Ok(()))));
    let (mut reader, writer) = std::io::pipe()?;
    let format = FormatId("application/x-custom".into());
    run.transfers
        .handle(ClipboardEvent::SourceSend(id, format, Ok(writer.into())));
    move_bytes(&run.watches);
    let mut bytes = Vec::new();
    let _count = reader.read_to_end(&mut bytes)?;
    assert_eq!(bytes, [0, 255, 1]);
    Ok(())
}

#[test]
fn a_paste_whose_pipe_failed_leaves_the_selection_for_the_next_paste() -> TestResult {
    let run = Run::new()?;
    let set = set_selection(&run.transfers, TransferData::text("copied"));
    run.transfers.submit(&run.records);
    let (id, _) = set_request(run.transfers.take_requests())?;
    run.transfers
        .handle(ClipboardEvent::SelectionSet(id, Ok(())));
    assert!(matches!(*set.borrow(), Some(Ok(()))));
    let format = FormatId("text/plain".into());
    let failed = std::io::Error::from(std::io::ErrorKind::InvalidInput);
    run.transfers
        .handle(ClipboardEvent::SourceSend(id, format.clone(), Err(failed)));
    assert!(run.watches.take_requests().is_empty());
    let (mut reader, writer) = std::io::pipe()?;
    run.transfers
        .handle(ClipboardEvent::SourceSend(id, format, Ok(writer.into())));
    move_bytes(&run.watches);
    let mut bytes = Vec::new();
    let _count = reader.read_to_end(&mut bytes)?;
    assert_eq!(bytes, b"copied");
    Ok(())
}

#[test]
fn a_refused_selection_reports_the_refusal_and_sends_nothing() -> TestResult {
    let run = Run::new()?;
    let set = set_selection(&run.transfers, TransferData::text("copied"));
    run.transfers.submit(&run.records);
    let (id, _) = set_request(run.transfers.take_requests())?;
    let refusal = SelectionRefusal::NoInput;
    run.transfers
        .handle(ClipboardEvent::SelectionSet(id, Err(refusal)));
    assert!(matches!(*set.borrow(), Some(Err(Error::SetSelection(reason))) if reason == refusal));
    let (mut reader, writer) = std::io::pipe()?;
    let format = FormatId("text/plain".into());
    run.transfers
        .handle(ClipboardEvent::SourceSend(id, format, Ok(writer.into())));
    assert!(run.watches.take_requests().is_empty());
    let mut bytes = Vec::new();
    let _count = reader.read_to_end(&mut bytes)?;
    assert!(bytes.is_empty());
    Ok(())
}

#[test]
fn a_send_after_the_selection_was_cancelled_writes_nothing() -> TestResult {
    let run = Run::new()?;
    let set = set_selection(&run.transfers, TransferData::text("copied"));
    run.transfers.submit(&run.records);
    let (id, _) = set_request(run.transfers.take_requests())?;
    run.transfers
        .handle(ClipboardEvent::SelectionSet(id, Ok(())));
    assert!(matches!(*set.borrow(), Some(Ok(()))));
    run.transfers.handle(ClipboardEvent::SourceCancelled(id));
    let (mut reader, writer) = std::io::pipe()?;
    let format = FormatId("text/plain".into());
    run.transfers
        .handle(ClipboardEvent::SourceSend(id, format, Ok(writer.into())));
    assert!(run.watches.take_requests().is_empty());
    let mut bytes = Vec::new();
    let _count = reader.read_to_end(&mut bytes)?;
    assert!(bytes.is_empty());
    Ok(())
}

#[test]
fn a_read_receives_every_byte_in_the_first_offered_format() -> TestResult {
    let run = Run::new()?;
    run.transfers.handle(offer(&["image/png", "text/plain"]));
    let read = run.read(&["text/html", "text/plain", "image/png"]);
    let (id, format) = receive_request(run.transfers.take_requests())?;
    assert_eq!(format, FormatId("text/plain".into()));
    let expected: Vec<u8> = (0..=u8::MAX).cycle().take(5000).collect();
    let (reader, mut writer) = std::io::pipe()?;
    writer.write_all(&expected)?;
    drop(writer);
    run.transfers
        .handle(ClipboardEvent::ReceiveStarted(id, Ok(reader.into())));
    move_bytes(&run.watches);
    let received = read
        .borrow_mut()
        .take()
        .ok_or(TestError::Missing("the bytes"))??;
    assert_eq!(received.mime_type, "text/plain");
    assert_eq!(received.bytes, expected);
    Ok(())
}

#[test]
fn a_read_fails_without_a_matching_offer_or_an_open_surface() -> TestResult {
    let mut run = Run::new()?;
    let empty = run.read(&["text/plain"]);
    assert!(matches!(*empty.borrow(), Some(Err(Error::ClipboardEmpty))));
    run.transfers.handle(offer(&["image/png"]));
    let other = run.read(&["text/plain"]);
    assert!(matches!(
        *other.borrow(),
        Some(Err(Error::NoRequestedFormat))
    ));
    let refused = run.read(&["image/png"]);
    let (id, _) = receive_request(run.transfers.take_requests())?;
    let refusal = Err(ReceiveRefusal::Empty);
    run.transfers
        .handle(ClipboardEvent::ReceiveStarted(id, refusal));
    assert!(matches!(
        *refused.borrow(),
        Some(Err(Error::ClipboardEmpty))
    ));
    let _destroy_request = run.records.handle(run.surface, SurfaceEvent::Closed);
    let closed = run.read(&["image/png"]);
    assert!(matches!(
        *closed.borrow(),
        Some(Err(Error::Cancelled(Cancellation::SurfaceClosed)))
    ));
    assert!(run.transfers.take_requests().is_empty());
    Ok(())
}

#[test]
fn a_retired_surface_cancels_its_reads_and_their_watches() -> TestResult {
    let run = Run::new()?;
    run.transfers.handle(offer(&["text/plain"]));
    let read = run.read(&["text/plain"]);
    let (id, _) = receive_request(run.transfers.take_requests())?;
    let (reader, _writer) = std::io::pipe()?;
    run.transfers
        .handle(ClipboardEvent::ReceiveStarted(id, Ok(reader.into())));
    assert!(matches!(
        run.watches.take_requests()[..],
        [WatchRequest::Watch(..)]
    ));
    run.transfers.forget_surface(run.surface);
    assert!(matches!(
        *read.borrow(),
        Some(Err(Error::Cancelled(Cancellation::SurfaceClosed)))
    ));
    assert!(matches!(
        run.watches.take_requests()[..],
        [WatchRequest::Unwatch(..)]
    ));
    Ok(())
}

#[test]
fn closing_cancels_queued_and_later_requests() -> TestResult {
    let run = Run::new()?;
    let queued = set_selection(&run.transfers, TransferData::text("queued"));
    run.transfers.close();
    let later = set_selection(&run.transfers, TransferData::text("later"));
    for outcome in [queued, later] {
        assert!(matches!(
            *outcome.borrow(),
            Some(Err(Error::Cancelled(Cancellation::AppClosed)))
        ));
    }
    let cancelled = Rc::new(Cell::new(false));
    let reported = cancelled.clone();
    let clipboard = run.transfers.clipboard();
    clipboard.read(run.surface, ["text/plain"], move |result| {
        reported.set(matches!(
            result,
            Err(Error::Cancelled(Cancellation::AppClosed))
        ));
    });
    assert!(cancelled.get());
    Ok(())
}
