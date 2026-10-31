// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::DragRequest;
use std::{cell::Cell, rc::Rc};

fn native_source(adapter: &TextTransfers, view: &mut View) -> Result<DragRequest, TestError> {
    let id = field_id(view)?;
    assert_eq!(
        view.accessibility_action(ActionRequest {
            target: id,
            action: AccessibilityAction::SetTextSelection(TextSelection {
                anchor: 4,
                focus: 7
            }),
        }),
        Ok(ActionOutcome::Performed)
    );
    view.input(InputEvent::Press(Point::new(51.dp(), 5.dp())));
    let gesture = DragGesture {
        surface: 0,
        origin: Point::new(51.dp(), 5.dp()),
        position: Point::new(150.dp(), 5.dp()),
    };
    adapter
        .source(&gesture, 1, view)
        .ok_or(TestError::NoDragSource)
}

/// Runs the start callback of `request` with `view`, as the host does after the drag started.
fn start_drag(request: &mut DragRequest, view: &mut View) -> Result<(), TestError> {
    let started = request.started.take().ok_or(TestError::NoStartCallback)?;
    started(view);
    Ok(())
}

/// Runs the receipt callback of `request` for transfer `id`, as the host does when the drag sends that transfer to
/// a destination in this app.
fn link_receipt(request: &mut DragRequest, id: TransferId) -> Result<(), TestError> {
    let receipt = request
        .received
        .as_mut()
        .ok_or(TestError::NoReceiptCallback)?;
    receipt(id);
    Ok(())
}

#[test]
fn correlated_native_detour_moves_duplicate_range_once_and_finish_does_not_delete_again()
-> Result<(), TestError> {
    let text = State::new("abc abc xyz".to_string());
    let value = text.clone();
    let changes = Rc::new(Cell::new(0));
    let count = changes.clone();
    let mut view = View::new(lavashell_test_fixtures::facts(), move |ui| {
        let value = value.clone();
        let count = count.clone();
        ui.text_area(
            value.get(),
            TextFieldOptions::default(),
            Modifier::new()
                .accessible_name("field")
                .size(140.dp(), 60.dp()),
            move |text| {
                value.set(text);
                count.set(count.get() + 1);
            },
        );
    });
    let update = (view.frame(WINDOW, &mut Metrics)).without_failures()?;
    assert!(!update.changes.is_empty());
    let mut adapter = TextTransfers::default();
    let mut request = native_source(&adapter, &mut view)?;
    start_drag(&mut request, &mut view)?;
    let info = offer(3, Point::new(111.dp(), 5.dp()), DragAction::Move);
    assert!(
        adapter
            .drag(&DragEvent::Enter(info.clone()), 1, &view)
            .is_some()
    );
    assert!(
        adapter
            .drag(&DragEvent::Drop(info.clone()), 1, &view)
            .is_none()
    );
    link_receipt(&mut request, info.id)?;
    assert_eq!(changes.get(), 0);
    assert!(adapter.receive(received(&info, b"abc")?, &mut view));
    assert_eq!(text.get(), "abc  xyzabc");
    assert_eq!(changes.get(), 1);
    (request.done)(Ok(DragAction::Move), &mut view);
    assert!(!adapter.receive(received(&info, b"abc")?, &mut view));
    assert_eq!(text.get(), "abc  xyzabc");
    assert_eq!(changes.get(), 1);
    Ok(())
}

#[test]
fn equal_foreign_text_without_native_correlation_does_not_move_active_source()
-> Result<(), TestError> {
    for device in [1, 2] {
        let (mut view, text, _) = editor()?;
        text.set("abc abc xyz".to_string());
        let update = (view.frame(WINDOW, &mut Metrics)).without_failures()?;
        assert!(!update.changes.is_empty());
        let mut adapter = TextTransfers::default();
        let mut request = native_source(&adapter, &mut view)?;
        start_drag(&mut request, &mut view)?;
        let info = offer(3, Point::new(111.dp(), 5.dp()), DragAction::Move);
        assert!(
            adapter
                .drag(&DragEvent::Enter(info.clone()), device, &view)
                .is_some()
        );
        assert!(
            adapter
                .drag(&DragEvent::Drop(info.clone()), device, &view)
                .is_none()
        );
        assert_eq!(
            adapter.receive(received(&info, b"abc")?, &mut view),
            device != 1
        );
        assert_eq!(
            text.get(),
            if device == 1 {
                "abc abc xyz"
            } else {
                "abc abc xyzabc"
            }
        );
        (request.done)(Err(crate::Cancellation::SourceCancelled.into()), &mut view);
        assert_eq!(
            text.get(),
            if device == 1 {
                "abc abc xyz"
            } else {
                "abc abc xyzabc"
            }
        );
    }
    Ok(())
}

#[test]
fn cancelled_correlated_source_rejects_receipt_without_deleting_or_inserting()
-> Result<(), TestError> {
    let (mut view, text, _) = editor()?;
    text.set("abc abc xyz".to_string());
    let update = (view.frame(WINDOW, &mut Metrics)).without_failures()?;
    assert!(!update.changes.is_empty());
    let mut adapter = TextTransfers::default();
    let mut request = native_source(&adapter, &mut view)?;
    start_drag(&mut request, &mut view)?;
    let info = offer(3, Point::new(111.dp(), 5.dp()), DragAction::Move);
    assert!(
        adapter
            .drag(&DragEvent::Enter(info.clone()), 1, &view)
            .is_some()
    );
    assert!(
        adapter
            .drag(&DragEvent::Drop(info.clone()), 1, &view)
            .is_none()
    );
    link_receipt(&mut request, info.id)?;
    (request.done)(Err(crate::Cancellation::SourceCancelled.into()), &mut view);
    assert!(!adapter.receive(received(&info, b"abc")?, &mut view));
    assert_eq!(text.get(), "abc abc xyz");
    Ok(())
}

#[test]
fn correlated_native_self_range_receipt_is_a_noop_including_source_finish() -> Result<(), TestError>
{
    let (mut view, text, _) = editor()?;
    text.set("abc abc xyz".to_string());
    let update = (view.frame(WINDOW, &mut Metrics)).without_failures()?;
    assert!(!update.changes.is_empty());
    let mut adapter = TextTransfers::default();
    let mut request = native_source(&adapter, &mut view)?;
    start_drag(&mut request, &mut view)?;
    let info = offer(3, Point::new(61.dp(), 5.dp()), DragAction::Move);
    assert!(
        adapter
            .drag(&DragEvent::Enter(info.clone()), 1, &view)
            .is_some()
    );
    assert!(
        adapter
            .drag(&DragEvent::Drop(info.clone()), 1, &view)
            .is_none()
    );
    link_receipt(&mut request, info.id)?;
    assert!(adapter.receive(received(&info, b"abc")?, &mut view));
    (request.done)(Ok(DragAction::Move), &mut view);
    assert_eq!(text.get(), "abc abc xyz");
    Ok(())
}

#[test]
fn cancelled_correlated_source_also_rejects_receipt_in_another_window() -> Result<(), TestError> {
    let (mut origin, text, _) = editor()?;
    text.set("abc abc xyz".to_string());
    let update = (origin.frame(WINDOW, &mut Metrics)).without_failures()?;
    assert!(!update.changes.is_empty());
    let (mut destination, target, _) = editor()?;
    let mut adapter = TextTransfers::default();
    let mut request = native_source(&adapter, &mut origin)?;
    start_drag(&mut request, &mut origin)?;
    let mut info = offer(3, Point::new(31.dp(), 5.dp()), DragAction::Move);
    info.surface = 1;
    assert!(
        adapter
            .drag(&DragEvent::Enter(info.clone()), 1, &destination)
            .is_some()
    );
    assert!(
        adapter
            .drag(&DragEvent::Drop(info.clone()), 1, &destination)
            .is_none()
    );
    link_receipt(&mut request, info.id)?;
    (request.done)(
        Err(crate::Cancellation::SourceCancelled.into()),
        &mut origin,
    );
    assert!(!adapter.receive(received(&info, b"abc")?, &mut destination));
    assert_eq!(text.get(), "abc abc xyz");
    assert_eq!(target.get(), "abc def");
    Ok(())
}
