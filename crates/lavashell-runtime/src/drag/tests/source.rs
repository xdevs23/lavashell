// SPDX-License-Identifier: GPL-3.0-only
use super::*;

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
        Some(Err(Error::WithoutTransfer))
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
