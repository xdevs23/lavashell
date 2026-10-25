// SPDX-License-Identifier: GPL-3.0-only
use super::selection::{Shaped, editor, point, select, select_range};
use super::*;

const WINDOW: Viewport = Viewport::new(Size::new(Dp::new(300.0), Dp::new(100.0)), Density::ONE);

#[test]
fn source_capture_and_copy_do_not_delete_and_move_uses_original_duplicate_range() -> TestResult {
    let mut scene = Scene::default();
    let (mut view, state, changes) = editor(&mut scene, "abc abc xyz", false)?;
    let id = select(&mut view, 4, 7)?;
    let source = view
        .text_selection_source(id)
        .required("a selection source")?;
    assert_eq!(source.text(), "abc");
    assert!(source.can_move());
    assert_eq!(
        view.text_selection_source_at(point(5))
            .required("a selection source")?
            .text(),
        "abc"
    );
    assert!(view.text_selection_source_at(point(0)).is_none());
    let repeated = source.clone();
    assert_eq!(repeated.text(), "abc");
    assert_eq!(state.get(), "abc abc xyz");
    assert_eq!(changes.get(), 0);
    select_range(&mut view, 0, 3)?;
    view.focus = None;
    assert!(view.complete_text_move(&source));
    assert_eq!(state.get(), "abc  xyz");
    assert_eq!(changes.get(), 1);
    assert!(!view.complete_text_move(&repeated));
    assert_eq!(changes.get(), 1);
    Ok(())
}

#[test]
fn source_move_preserves_combining_and_emoji_units() -> TestResult {
    let mut scene = Scene::default();
    let (mut view, state, changes) = editor(&mut scene, "e\u{301}👩\u{200d}💻z", false)?;
    let id = select(&mut view, 3, 14)?;
    let source = view
        .text_selection_source(id)
        .required("a selection source")?;
    assert_eq!(source.text(), "👩\u{200d}💻");
    assert!(view.complete_text_move(&source));
    assert_eq!(state.get(), "e\u{301}z");
    assert_eq!(changes.get(), 1);
    Ok(())
}

#[test]
fn read_only_sources_support_copy_and_never_delete() -> TestResult {
    let mut scene = Scene::default();
    let (mut view, state, changes) = editor(&mut scene, "abc def", true)?;
    let id = select(&mut view, 0, 3)?;
    let source = view
        .text_selection_source(id)
        .required("a selection source")?;
    assert_eq!(source.text(), "abc");
    assert!(!source.can_move());
    assert!(!view.complete_text_move(&source));
    assert_eq!(state.get(), "abc def");
    assert_eq!(changes.get(), 0);
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), |ui| {
        ui.text("abc def", Modifier::new().user_select(true).tag("field"));
    });
    advance_with(&mut scene, &mut view, WINDOW, &mut Shaped)?;
    let id = select(&mut view, 0, 3)?;
    let source = view
        .text_selection_source(id)
        .required("a selection source")?;
    assert_eq!(source.text(), "abc");
    assert!(!source.can_move());
    assert!(!view.complete_text_move(&source));
    assert_eq!(view.copy_selection(false).as_deref(), Some("abc"));
    Ok(())
}

#[test]
fn source_changes_reject_move_before_and_after_composition() -> TestResult {
    let mut scene = Scene::default();
    let (mut view, state, changes) = editor(&mut scene, "abc def", false)?;
    let id = select(&mut view, 0, 3)?;
    let source = view
        .text_selection_source(id)
        .required("a selection source")?;
    state.set("external text".to_string());
    assert!(!view.complete_text_move(&source));
    advance_with(&mut scene, &mut view, WINDOW, &mut Shaped)?;
    assert!(!view.complete_text_move(&source));
    assert_eq!(state.get(), "external text");
    assert_eq!(changes.get(), 0);
    Ok(())
}

#[derive(Clone, Copy)]
struct Availability {
    enabled: bool,
    present: bool,
    modal: bool,
}

#[test]
fn disabled_removed_and_modal_excluded_sources_reject_completed_move() -> TestResult {
    for availability in [
        Availability {
            enabled: false,
            present: true,
            modal: false,
        },
        Availability {
            enabled: true,
            present: false,
            modal: false,
        },
        Availability {
            enabled: true,
            present: true,
            modal: true,
        },
    ] {
        let state = State::new(Availability {
            enabled: true,
            present: true,
            modal: false,
        });
        let current = state.clone();
        let value = State::new("abc def".to_string());
        let text = value.clone();
        let mut scene = Scene::default();
        let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
            let availability = current.get();
            ui.key("source", Modifier::new(), |ui| {
                if availability.present {
                    let text = text.clone();
                    ui.text_area(
                        text.get(),
                        TextFieldOptions::default(),
                        Modifier::new()
                            .enabled(availability.enabled)
                            .tag("field")
                            .size(240.dp(), 60.dp()),
                        move |value| text.set(value),
                    );
                }
            });
            if availability.modal {
                ui.layout(Arrangement::Overlay, Modifier::new().modal(), |ui| {
                    ui.text("dialog", Modifier::new());
                });
            }
        });
        advance_with(&mut scene, &mut view, WINDOW, &mut Shaped)?;
        let id = select(&mut view, 0, 3)?;
        let source = view
            .text_selection_source(id)
            .required("a selection source")?;
        state.set(availability);
        assert!(!view.complete_text_move(&source));
        advance_with(&mut scene, &mut view, WINDOW, &mut Shaped)?;
        assert!(!view.complete_text_move(&source));
        assert!(view.text_selection_source(id).is_none());
        assert_eq!(value.get(), "abc def");
    }
    Ok(())
}

#[test]
fn fresh_read_only_metadata_rejects_move_without_a_visual_frame() -> TestResult {
    let read_only = State::new(false);
    let read = read_only.clone();
    let value = State::new("abc def".to_string());
    let current = value.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let read = read.clone();
        let current = current.clone();
        ui.text_area(
            current.get(),
            TextFieldOptions::default(),
            Modifier::new()
                .tag("field")
                .semantics_dynamic(move |semantics| {
                    semantics.state.form.read_only = read.get();
                }),
            move |value| current.set(value),
        );
    });
    advance_with(&mut scene, &mut view, WINDOW, &mut Shaped)?;
    let id = select(&mut view, 0, 3)?;
    let source = view
        .text_selection_source(id)
        .required("a selection source")?;
    assert!(source.can_move());
    read_only.set(true);
    assert!(!view.complete_text_move(&source));
    assert_eq!(value.get(), "abc def");
    Ok(())
}
