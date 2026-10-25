// SPDX-License-Identifier: GPL-3.0-only
use super::selection::{Shaped, editor, point, select};
use super::*;

#[test]
fn native_detour_moves_exact_duplicate_range_once_before_source_finish() -> TestResult {
    let mut scene = Scene::default();
    let (mut view, state, changes) = editor(&mut scene, "abc abc xyz", false)?;
    let id = select(&mut view, 4, 7)?;
    view.input(InputEvent::Press(point(5)));
    let source = view
        .text_selection_source_at(point(5))
        .required("a selection source")?;
    assert!(!view.text_source_contains(&source, Point::new(260.dp(), 5.dp())));
    assert!(view.text_source_drag_ready(&source, Point::new(260.dp(), 5.dp())));
    let edit = view.edits.get_mut(&id).required("an edit of the field")?;
    edit.cancel_drag();
    let target = view.text_drop_target(point(11)).required("a drop target")?;
    assert_eq!(state.get(), "abc abc xyz");
    assert_eq!(changes.get(), 0);
    assert!(view.move_text_at(&source, &target));
    assert_eq!(state.get(), "abc  xyzabc");
    assert_eq!(changes.get(), 1);
    assert!(!view.move_text_at(&source, &target));
    assert!(!view.complete_text_move(&source));
    advance_with(
        &mut scene,
        &mut view,
        Viewport::new(Size::new(300.dp(), 100.dp()), Density::ONE),
        &mut Shaped,
    )?;
    assert!(!view.complete_text_move(&source));
    assert_eq!(state.get(), "abc  xyzabc");
    assert_eq!(changes.get(), 1);
    Ok(())
}

#[test]
fn native_move_preserves_shaped_units_and_self_range_is_a_noop() -> TestResult {
    let mut scene = Scene::default();
    let (mut view, state, changes) = editor(&mut scene, "e\u{301}👩\u{200d}💻z", false)?;
    let id = select(&mut view, 3, 14)?;
    let source = view
        .text_selection_source(id)
        .required("a selection source")?;
    let target = view.text_drop_target(point(2)).required("a drop target")?;
    assert!(view.move_text_at(&source, &target));
    assert_eq!(state.get(), "e\u{301}👩\u{200d}💻z");
    assert_eq!(changes.get(), 0);
    let target = view.text_drop_target(point(0)).required("a drop target")?;
    assert!(view.move_text_at(&source, &target));
    assert_eq!(state.get(), "👩\u{200d}💻e\u{301}z");
    assert_eq!(changes.get(), 1);
    Ok(())
}

#[test]
fn native_move_rejects_changed_and_read_only_sources() -> TestResult {
    for read_only in [true, false] {
        let mut scene = Scene::default();
        let (mut view, state, changes) = editor(&mut scene, "abc def", read_only)?;
        let id = select(&mut view, 0, 3)?;
        let source = view
            .text_selection_source(id)
            .required("a selection source")?;
        let target = view.text_drop_target(point(7));
        if read_only {
            assert!(target.is_none());
        } else {
            state.set("external text".to_string());
            assert!(!view.move_text_at(&source, &target.required("a drop target")?));
            assert_eq!(state.get(), "external text");
        }
        assert_eq!(changes.get(), 0);
    }
    Ok(())
}
