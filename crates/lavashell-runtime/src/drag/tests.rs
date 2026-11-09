// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::clipboard::tests::move_bytes;
use lavashell_core::geometry::{Point, Size};
use lavashell_core::preferences::PreferenceOverrides;
use lavashell_core::transfer::{
    Cancellation, DragAction, DragActions, DragOfferInfo, TransferData, TransferError,
};
use lavashell_core::units::{Density, DpExt};
use lavashell_platform::{
    Button, DragDevice, DragReceiveRefusal, DragRefusal, InputSerial, PressState, SurfaceEvent,
    WatchRequest,
};
use lavashell_test_fixtures::FrameFailure;
use std::cell::Cell;
use std::io::{self, Write};

#[derive(Debug, thiserror::Error)]
pub(super) enum TestError {
    #[error("the runtime failed")]
    Runtime(#[from] Error),
    #[error("the transfer data is not valid")]
    Transfer(#[from] TransferError),
    #[error("a frame failed")]
    Frame(#[from] FrameFailure),
    #[error("a pipe failed")]
    Pipe(#[from] io::Error),
    #[error("{0} is missing")]
    Missing(&'static str),
}

type TestResult = Result<(), TestError>;

type Outcome = Rc<RefCell<Option<Result<DragAction, Error>>>>;

const DEVICE: DragDevice = DragDevice(1);
const POINTER: PointerDevice = PointerDevice(1);
const SERIAL: InputSerial = InputSerial(1);
const PNG: &str = "image/png";
const OFFER: TransferId = TransferId(7);

struct Run {
    watches: Watches,
    drags: Drags,
    records: SurfaceRecords,
    surface: SurfaceId,
}

impl Run {
    fn new() -> Result<Self, TestError> {
        let watches = Watches::default();
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
            drags: Drags::new(&watches),
            watches,
            records,
            surface,
        })
    }

    fn target(&mut self, event: DragTargetEvent) {
        self.drags.handle_target(event, &mut self.records);
    }

    fn source(&mut self, event: DragSourceEvent) {
        self.drags.handle_source(event, &mut self.records);
    }

    fn pointer(&mut self, event: &DeviceEvent) -> bool {
        (self.drags).track_gesture(self.surface, event, &mut self.records)
    }

    /// Enters the offer `OFFER` in PNG with copy over the surface.
    fn enter(&mut self) {
        let offer = DragOfferInfo {
            id: OFFER,
            surface: self.surface,
            position: Point::new(10.dp(), 10.dp()),
            mime_types: vec![PNG.into()],
            actions: DragActions::new([DragAction::Copy]),
            selected_action: None,
        };
        self.target(DragTargetEvent::Enter {
            device: DEVICE,
            offer,
        });
    }

    /// Enters and drops the offer `OFFER`, which the app accepts, and returns the drops reported to the app, which
    /// uses each of them.
    fn drop_png(&mut self) -> Rc<RefCell<Vec<DropEvent>>> {
        let _events = accept_png(&mut self.drags);
        let drops = Rc::new(RefCell::new(Vec::new()));
        let seen = drops.clone();
        self.drags.on_drop(move |event, _| {
            seen.borrow_mut().push(event);
            true
        });
        self.enter();
        self.target(DragTargetEvent::ActionSelected(
            OFFER,
            Some(DragAction::Copy),
        ));
        self.target(DragTargetEvent::Drop(OFFER));
        drops
    }

    /// Starts `request` through a primary press and a motion, and returns the drag of the start request.
    fn start(&mut self, request: DragRequest) -> Result<DragSourceId, TestError> {
        let mut request = Some(request);
        self.drags.on_drag_source(move |_, _| request.take());
        assert!(!self.pointer(&press(Button::Primary)));
        assert!(self.pointer(&motion()));
        let Some(DragSourceRequest::Start { drag, .. }) = self.drags.take_source_requests().pop()
        else {
            return Err(TestError::Missing("a start request"));
        };
        Ok(drag)
    }

    /// Reports that the drag `drag` started, and that a destination reads it in `format` from a new pipe.
    fn send(&mut self, drag: DragSourceId, format: &str) -> Result<io::PipeReader, TestError> {
        let (reader, writer) = io::pipe()?;
        self.source(DragSourceEvent::Started(drag, Ok(DEVICE)));
        self.source(DragSourceEvent::Send {
            drag,
            format: FormatId(format.into()),
            pipe: Ok(writer.into()),
            local: None,
        });
        Ok(reader)
    }
}

fn press(button: Button) -> DeviceEvent {
    DeviceEvent::Button {
        pointer: POINTER,
        button,
        state: PressState::Pressed,
        at: Point::new(5.dp(), 5.dp()),
        serial: SERIAL,
    }
}

fn motion() -> DeviceEvent {
    DeviceEvent::PointerMotion {
        pointer: POINTER,
        at: Point::new(40.dp(), 5.dp()),
    }
}

fn copy_png() -> DropAcceptance {
    DropAcceptance {
        mime_type: PNG.into(),
        action: DragAction::Copy,
    }
}

/// Records each drag event and accepts every drag in PNG with copy.
fn accept_png(drags: &mut Drags) -> Rc<RefCell<Vec<DragEvent>>> {
    let events = Rc::new(RefCell::new(Vec::new()));
    let seen = events.clone();
    drags.on_drag(move |event, _| {
        seen.borrow_mut().push(event);
        Some(copy_png())
    });
    events
}

/// A drag of PNG data with copy, whose result goes to `outcome`.
fn png_drag(outcome: &Outcome) -> Result<DragRequest, TestError> {
    let data = TransferData::single(PNG, vec![1, 2, 3])?;
    let reported = outcome.clone();
    let actions = DragActions::new([DragAction::Copy]);
    Ok(DragRequest::new(data, actions, move |result, _| {
        *reported.borrow_mut() = Some(result);
    }))
}

/// The acceptance in each answer of `requests`.
fn answers(requests: Vec<DragTargetRequest>) -> Result<Vec<Option<DropAcceptance>>, TestError> {
    (requests.into_iter())
        .map(|request| {
            let DragTargetRequest::Answer(_, acceptance) = request else {
                return Err(TestError::Missing("an answer"));
            };
            Ok(acceptance)
        })
        .collect()
}

#[test]
fn an_offer_is_answered_once_with_the_acceptance_that_its_actions_allow() -> TestResult {
    let mut run = Run::new()?;
    let _events = accept_png(&mut run.drags);
    run.enter();
    assert_eq!(
        answers(run.drags.take_target_requests())?,
        [Some(copy_png())]
    );
    let moved = DragActions::new([DragAction::Move]);
    run.target(DragTargetEvent::ActionsChanged(OFFER, moved));
    run.target(DragTargetEvent::Motion(OFFER, Point::new(20.dp(), 10.dp())));
    assert_eq!(answers(run.drags.take_target_requests())?, [None]);
    Ok(())
}

#[test]
fn a_drop_without_the_accepted_action_is_cancelled_and_released() -> TestResult {
    let mut run = Run::new()?;
    let events = accept_png(&mut run.drags);
    run.enter();
    run.target(DragTargetEvent::Drop(OFFER));
    assert!(matches!(
        events.borrow().as_slice(),
        [
            DragEvent::Enter(_),
            DragEvent::Drop(_),
            DragEvent::Cancelled { .. }
        ]
    ));
    assert!(matches!(
        run.drags.take_target_requests().as_slice(),
        [
            DragTargetRequest::Answer(..),
            DragTargetRequest::Release(OFFER)
        ]
    ));
    Ok(())
}

#[test]
fn an_unreadable_drop_is_reported_to_the_drop_observer_and_released() -> TestResult {
    let mut run = Run::new()?;
    let drops = run.drop_png();
    let receive = run.drags.take_target_requests().pop();
    assert!(matches!(
        receive,
        Some(DragTargetRequest::Receive(OFFER, FormatId(format))) if format == PNG
    ));
    let refusal = DragReceiveRefusal::Pipe(io::ErrorKind::BrokenPipe.into());
    run.target(DragTargetEvent::ReceiveStarted(OFFER, Err(refusal)));
    assert!(matches!(
        drops.borrow().as_slice(),
        [DropEvent {
            data: Err(Error::ReceiveDrag(DragReceiveRefusal::Pipe(_))),
            ..
        }]
    ));
    assert!(matches!(
        run.drags.take_target_requests().as_slice(),
        [DragTargetRequest::Release(OFFER)]
    ));
    Ok(())
}

#[test]
fn a_received_drop_that_the_app_used_is_finished() -> TestResult {
    let mut run = Run::new()?;
    let drops = run.drop_png();
    let _requests = run.drags.take_target_requests();
    let (reader, mut writer) = io::pipe()?;
    writer.write_all(&[1, 2, 3])?;
    drop(writer);
    run.target(DragTargetEvent::ReceiveStarted(OFFER, Ok(reader.into())));
    move_bytes(&run.watches);
    run.drags.submit(&mut run.records);
    assert!(matches!(
        drops.borrow().as_slice(),
        [DropEvent { data: Ok(data), .. }] if data.bytes == [1, 2, 3]
    ));
    assert!(matches!(
        run.drags.take_target_requests().as_slice(),
        [DragTargetRequest::Finish(OFFER)]
    ));
    Ok(())
}

#[test]
fn a_cancelled_drop_ends_its_read() -> TestResult {
    let mut run = Run::new()?;
    let drops = run.drop_png();
    let (reader, _writer) = io::pipe()?;
    run.target(DragTargetEvent::ReceiveStarted(OFFER, Ok(reader.into())));
    assert!(matches!(
        run.watches.take_requests()[..],
        [WatchRequest::Watch(..)]
    ));
    run.target(DragTargetEvent::Cancelled(OFFER));
    assert!(matches!(
        drops.borrow().as_slice(),
        [DropEvent {
            data: Err(Error::Cancelled(Cancellation::InputDeviceGone)),
            ..
        }]
    ));
    assert!(matches!(
        run.watches.take_requests()[..],
        [WatchRequest::Unwatch(..)]
    ));
    Ok(())
}

#[test]
fn a_drop_whose_read_did_not_start_is_cancelled_when_the_app_closes() -> TestResult {
    let mut run = Run::new()?;
    let drops = run.drop_png();
    run.drags.close(&mut run.records);
    assert!(matches!(
        drops.borrow().as_slice(),
        [DropEvent {
            data: Err(Error::Cancelled(Cancellation::AppClosed)),
            ..
        }]
    ));
    Ok(())
}

#[test]
fn a_gesture_starts_the_drag_returned_by_the_app_once_the_platform_reports_it() -> TestResult {
    let mut run = Run::new()?;
    let started = Rc::new(Cell::new(false));
    let result = Rc::new(RefCell::new(None));
    let reported_start = started.clone();
    let request = png_drag(&result)?.on_start(move |_| reported_start.set(true));
    let drag = run.start(request)?;
    assert!(!started.get());
    run.source(DragSourceEvent::Started(drag, Ok(DEVICE)));
    assert!(started.get());
    run.source(DragSourceEvent::Finished(drag));
    assert!(matches!(
        *result.borrow(),
        Some(Err(Error::DragWithoutTransfer))
    ));
    assert!(!run.pointer(&motion()));
    Ok(())
}

#[test]
fn a_gesture_starts_no_drag_after_a_release_a_cancel_or_another_first_button() -> TestResult {
    let mut run = Run::new()?;
    let result = Rc::new(RefCell::new(None));
    let mut request = Some(png_drag(&result)?);
    run.drags.on_drag_source(move |_, _| request.take());
    let release = DeviceEvent::Button {
        pointer: POINTER,
        button: Button::Primary,
        state: PressState::Released,
        at: Point::new(5.dp(), 5.dp()),
        serial: SERIAL,
    };
    assert!(!run.pointer(&press(Button::Primary)));
    assert!(!run.pointer(&release));
    assert!(!run.pointer(&motion()));
    assert!(!run.pointer(&press(Button::Primary)));
    run.drags.cancel_gesture(POINTER, run.surface);
    assert!(!run.pointer(&motion()));
    assert!(!run.pointer(&press(Button::Secondary)));
    assert!(!run.pointer(&press(Button::Primary)));
    assert!(!run.pointer(&motion()));
    assert!(run.drags.take_source_requests().is_empty());
    Ok(())
}

#[test]
fn a_refused_start_ends_the_drag_with_the_refusal() -> TestResult {
    let mut run = Run::new()?;
    let result = Rc::new(RefCell::new(None));
    let drag = run.start(png_drag(&result)?)?;
    run.source(DragSourceEvent::Started(drag, Err(DragRefusal::NoPress)));
    assert!(matches!(
        *result.borrow(),
        Some(Err(Error::StartDrag(DragRefusal::NoPress)))
    ));
    Ok(())
}

#[test]
fn a_send_in_a_format_that_the_drag_did_not_offer_fails_the_drag() -> TestResult {
    let mut run = Run::new()?;
    let result = Rc::new(RefCell::new(None));
    let drag = run.start(png_drag(&result)?)?;
    let _reader = run.send(drag, "text/plain")?;
    run.source(DragSourceEvent::Finished(drag));
    assert!(matches!(
        *result.borrow(),
        Some(Err(Error::UnadvertisedFormat))
    ));
    Ok(())
}

#[test]
fn a_send_whose_pipe_the_platform_could_not_prepare_fails_the_drag() -> TestResult {
    let mut run = Run::new()?;
    let result = Rc::new(RefCell::new(None));
    let drag = run.start(png_drag(&result)?)?;
    run.source(DragSourceEvent::Started(drag, Ok(DEVICE)));
    run.source(DragSourceEvent::Send {
        drag,
        format: FormatId(PNG.into()),
        pipe: Err(io::ErrorKind::InvalidInput.into()),
        local: None,
    });
    run.source(DragSourceEvent::Finished(drag));
    assert!(matches!(*result.borrow(), Some(Err(Error::SendDrag(_)))));
    Ok(())
}

#[test]
fn a_send_to_a_drop_in_the_app_reports_the_local_receipt() -> TestResult {
    let mut run = Run::new()?;
    let result = Rc::new(RefCell::new(None));
    let receipts = Rc::new(RefCell::new(Vec::new()));
    let seen = receipts.clone();
    let request = png_drag(&result)?.on_local_receive(move |id| seen.borrow_mut().push(id));
    let drag = run.start(request)?;
    let (_reader, writer) = io::pipe()?;
    run.source(DragSourceEvent::Started(drag, Ok(DEVICE)));
    run.source(DragSourceEvent::Send {
        drag,
        format: FormatId(PNG.into()),
        pipe: Ok(writer.into()),
        local: Some(OFFER),
    });
    assert_eq!(*receipts.borrow(), [OFFER]);
    Ok(())
}

#[test]
fn a_cancelled_drag_ends_its_running_sends() -> TestResult {
    let mut run = Run::new()?;
    let result = Rc::new(RefCell::new(None));
    let drag = run.start(png_drag(&result)?)?;
    let _reader = run.send(drag, PNG)?;
    assert!(matches!(
        run.watches.take_requests()[..],
        [WatchRequest::Watch(..)]
    ));
    run.source(DragSourceEvent::Cancelled(
        drag,
        Cancellation::SourceCancelled,
    ));
    assert!(matches!(
        *result.borrow(),
        Some(Err(Error::Cancelled(Cancellation::SourceCancelled)))
    ));
    assert!(matches!(
        run.watches.take_requests()[..],
        [WatchRequest::Unwatch(..)]
    ));
    Ok(())
}

#[test]
fn a_closed_surface_cancels_the_drags_and_gestures_on_it() -> TestResult {
    let mut run = Run::new()?;
    let result = Rc::new(RefCell::new(None));
    let drag = run.start(png_drag(&result)?)?;
    assert!(!run.pointer(&press(Button::Primary)));
    let _destroy = run.records.handle(run.surface, SurfaceEvent::Closed);
    run.drags.forget_surface(run.surface, &mut run.records);
    assert!(matches!(
        run.drags.take_source_requests().as_slice(),
        [DragSourceRequest::Cancel(cancelled)] if *cancelled == drag
    ));
    assert!(matches!(
        *result.borrow(),
        Some(Err(Error::Cancelled(Cancellation::SurfaceClosed)))
    ));
    assert!(!run.pointer(&motion()));
    Ok(())
}

#[test]
fn a_closed_surface_reports_the_cancel_of_a_drop_whose_data_is_arriving() -> TestResult {
    let mut run = Run::new()?;
    let drops = run.drop_png();
    let (reader, _writer) = io::pipe()?;
    run.target(DragTargetEvent::ReceiveStarted(OFFER, Ok(reader.into())));
    let _destroy = run.records.handle(run.surface, SurfaceEvent::Closed);
    run.drags.forget_surface(run.surface, &mut run.records);
    assert!(matches!(
        drops.borrow().as_slice(),
        [DropEvent {
            data: Err(Error::Cancelled(Cancellation::SurfaceClosed)),
            ..
        }]
    ));
    Ok(())
}
