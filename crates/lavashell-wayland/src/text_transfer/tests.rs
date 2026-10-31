// SPDX-License-Identifier: GPL-3.0-only
use super::drag::TransferId;
use crate::test_error::TestError;
use crate::text_transfer::*;
use crate::{DragEvent, DragGesture, DragOfferInfo, DropEvent};
use lavashell_core::prelude::*;
use lavashell_core::surface::SurfaceId;
use lavashell_core::transfer::ReceivedData;
use lavashell_test_platform::WithoutFailures;

#[path = "native_source_tests.rs"]
mod native_source_tests;

struct Metrics;

impl TextMeasurer for Metrics {
    fn measure(&mut self, value: &str, _: &TextStyle<Dp>, width: Dp) -> Size {
        let natural: Dp = value.chars().map(|_| 10.dp()).sum();
        Size::new(natural.min(width), 20.dp())
    }
}

const WINDOW: Viewport = Viewport::new(Size::new(Dp::new(220.0), Dp::new(100.0)), Density::ONE);

fn editor() -> Result<(View, State<String>, NodeId), TestError> {
    let text = State::new("abc def".to_string());
    let value = text.clone();
    let mut view = View::new(lavashell_test_platform::facts(), move |ui| {
        let value = value.clone();
        ui.text_area(
            value.get(),
            TextFieldOptions::default(),
            Modifier::new()
                .accessible_name("field")
                .size(140.dp(), 60.dp()),
            move |text| value.set(text),
        );
    });
    let update = (view.frame(WINDOW, &mut Metrics)).without_failures()?;
    assert!(!update.changes.is_empty());
    let id = field_id(&view)?;
    Ok((view, text, id))
}

/// The id of the accessibility node named `field` in `view`, which is the text field of the test editor.
fn field_id(view: &View) -> Result<NodeId, TestError> {
    (view.accessibility().nodes.values())
        .find(|node| node.name.as_deref() == Some("field"))
        .map(|node| node.id)
        .ok_or(TestError::MissingField)
}

fn offer(id: u64, position: Point, action: DragAction) -> DragOfferInfo {
    DragOfferInfo {
        id: TransferId(id),
        surface: SurfaceId(0),
        position,
        mime_types: vec![
            "text/plain".to_string(),
            "text/plain;charset=utf-8".to_string(),
        ],
        actions: DragActions::new([action]),
        selected_action: Some(action),
    }
}

fn received(info: &DragOfferInfo, bytes: &[u8]) -> Result<DropEvent, TestError> {
    Ok(DropEvent {
        id: info.id,
        surface: info.surface,
        position: info.position,
        action: info.selected_action.ok_or(TestError::NoSelectedAction)?,
        data: Ok(ReceivedData {
            mime_type: TEXT_FORMATS[0].to_string(),
            bytes: bytes.to_vec(),
        }),
    })
}

#[test]
fn text_formats_preserve_utf8_and_choose_preferred_format() -> Result<(), TestError> {
    let data = text_data("雪🙂").ok_or(TestError::TextNotTransferable)?;
    assert_eq!(data.formats().collect::<Vec<_>>(), TEXT_FORMATS);
    for format in TEXT_FORMATS {
        let bytes = data.get(format).ok_or(TestError::MissingFormat(format))?;
        assert_eq!(bytes.as_ref(), "雪🙂".as_bytes());
    }
    let (view, _, _) = editor()?;
    let mut adapter = TextTransfers::default();
    let info = offer(1, Point::new(21.dp(), 5.dp()), DragAction::Copy);
    let acceptance = adapter
        .drag(&DragEvent::Enter(info), 1, &view)
        .ok_or(TestError::DragNotAccepted)?;
    assert_eq!(acceptance.mime_type, TEXT_FORMATS[0]);
    Ok(())
}

#[test]
fn physical_drop_freezes_destination_and_post_drop_leave_keeps_receiving_target()
-> Result<(), TestError> {
    let (mut view, text, _) = editor()?;
    let mut adapter = TextTransfers::default();
    let mut info = offer(1, Point::new(11.dp(), 5.dp()), DragAction::Copy);
    assert!(
        adapter
            .drag(&DragEvent::Enter(info.clone()), 1, &view)
            .is_some()
    );
    info.position = Point::new(31.dp(), 5.dp());
    assert!(
        adapter
            .drag(&DragEvent::Motion(info.clone()), 1, &view)
            .is_some()
    );
    assert!(
        adapter
            .drag(&DragEvent::Drop(info.clone()), 1, &view)
            .is_none()
    );
    assert!(
        adapter
            .drag(
                &DragEvent::Leave {
                    id: info.id,
                    surface: SurfaceId(0),
                },
                1,
                &view,
            )
            .is_none()
    );
    view.input(InputEvent::Press(Point::new(70.dp(), 5.dp())));
    view.input(InputEvent::Release(Point::new(70.dp(), 5.dp())));
    assert!(adapter.receive(received(&info, b"X")?, &mut view));
    assert_eq!(text.get(), "abcX def");
    assert!(!adapter.receive(received(&info, b"again")?, &mut view));
    Ok(())
}

#[test]
fn invalid_utf8_stale_destination_and_wrong_transfer_are_rejected() -> Result<(), TestError> {
    for invalid in [true, false] {
        let (mut view, text, _) = editor()?;
        let mut adapter = TextTransfers::default();
        let info = offer(1, Point::new(31.dp(), 5.dp()), DragAction::Copy);
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
        let unrelated = offer(2, Point::new(31.dp(), 5.dp()), DragAction::Copy);
        assert!(!adapter.receive(received(&unrelated, b"wrong")?, &mut view));
        assert!(adapter.has_pending(info.id));
        if invalid {
            assert!(!adapter.receive(received(&info, &[255])?, &mut view));
            assert_eq!(text.get(), "abc def");
        } else {
            text.set("external text".to_string());
            assert!(!adapter.receive(received(&info, b"stale")?, &mut view));
            assert_eq!(text.get(), "external text");
        }
    }
    Ok(())
}

#[test]
fn cancellation_and_application_acceptance_remove_text_fallback() -> Result<(), TestError> {
    let (mut view, text, _) = editor()?;
    let mut adapter = TextTransfers::default();
    let info = offer(1, Point::new(31.dp(), 5.dp()), DragAction::Copy);
    assert!(
        adapter
            .drag(&DragEvent::Enter(info.clone()), 1, &view)
            .is_some()
    );
    adapter.application_accepted(&DragEvent::Motion(info.clone()));
    assert!(
        adapter
            .drag(&DragEvent::Drop(info.clone()), 1, &view)
            .is_none()
    );
    assert!(!adapter.has_pending(info.id));
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
    assert!(
        adapter
            .drag(
                &DragEvent::Cancelled {
                    id: info.id,
                    surface: SurfaceId(0),
                },
                1,
                &view,
            )
            .is_none()
    );
    assert!(!adapter.receive(received(&info, b"cancelled")?, &mut view));
    assert_eq!(text.get(), "abc def");
    Ok(())
}

#[test]
fn rejected_physical_drop_retains_negotiated_ownership_until_receipt() -> Result<(), TestError> {
    let (mut view, text, _) = editor()?;
    let mut adapter = TextTransfers::default();
    let mut info = offer(1, Point::new(31.dp(), 5.dp()), DragAction::Copy);
    assert!(
        adapter
            .drag(&DragEvent::Enter(info.clone()), 1, &view)
            .is_some()
    );
    info.position = Point::new(180.dp(), 5.dp());
    assert!(adapter.owns(&DragEvent::Drop(info.clone())));
    assert!(
        adapter
            .drag(&DragEvent::Drop(info.clone()), 1, &view)
            .is_none()
    );
    assert!(
        adapter
            .drag(
                &DragEvent::Leave {
                    id: info.id,
                    surface: SurfaceId(0),
                },
                1,
                &view,
            )
            .is_none()
    );
    assert!(adapter.has_pending(info.id));
    assert!(!adapter.receive(received(&info, b"rejected")?, &mut view));
    assert!(!adapter.has_pending(info.id));
    assert_eq!(text.get(), "abc def");
    Ok(())
}

#[test]
fn selected_text_native_source_starts_only_after_leaving_local_field() -> Result<(), TestError> {
    let (mut view, text, id) = editor()?;
    assert_eq!(
        view.accessibility_action(ActionRequest {
            target: id,
            action: AccessibilityAction::SetTextSelection(TextSelection {
                anchor: 0,
                focus: 3
            })
        }),
        Ok(ActionOutcome::Performed)
    );
    view.input(InputEvent::Press(Point::new(11.dp(), 5.dp())));
    let adapter = TextTransfers::default();
    assert!(
        adapter
            .source(
                &DragGesture {
                    surface: SurfaceId(0),
                    origin: Point::new(11.dp(), 5.dp()),
                    position: Point::new(31.dp(), 5.dp())
                },
                1,
                &view
            )
            .is_none()
    );
    assert!(
        adapter
            .source(
                &DragGesture {
                    surface: SurfaceId(0),
                    origin: Point::new(11.dp(), 5.dp()),
                    position: Point::new(150.dp(), 5.dp())
                },
                1,
                &view
            )
            .is_some()
    );
    assert_eq!(text.get(), "abc def");
    Ok(())
}
