// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::accessibility::{
    AccessibilityAction, ActionKind, ActionOutcome, ActionRequest, TextSelection,
};
use std::{cell::Cell, rc::Rc};

const WINDOW: Viewport = Viewport::new(Size::new(Dp::new(300.0), Dp::new(100.0)), Density::ONE);

pub(super) struct Shaped;

impl TextMeasurer for Shaped {
    fn measure(&mut self, value: &str, _: &TextStyle<Dp>, width: Dp) -> Size {
        Size::new(repeated(10.dp(), value.chars().count()).min(width), 20.dp())
    }

    fn geometry(&mut self, value: &str, _: &TextStyle<Dp>, width: Dp) -> TextGeometry {
        let boundaries: Vec<usize> = if value == "e\u{301}👩\u{200d}💻z" {
            vec![0, 3, 14, 15]
        } else {
            value
                .char_indices()
                .map(|(byte, _)| byte)
                .chain([value.len()])
                .collect()
        };
        TextGeometry {
            carets: boundaries
                .into_iter()
                .enumerate()
                .map(|(column, index)| TextCaret {
                    index,
                    rect: Rect::new(repeated(10.dp(), column), 0.dp(), 1.dp(), 20.dp()),
                    line: 0,
                })
                .collect(),
            size: Size::new(width, 20.dp()),
            ..Default::default()
        }
    }
}

/// A view with one text area tagged "field", the state that contains the text of that text area,
/// and how often the text area has called its change callback.
pub(super) type Editor = (View, State<String>, Rc<Cell<usize>>);

pub(super) fn editor(
    scene: &mut Scene,
    initial: &str,
    read_only: bool,
) -> Result<Editor, TestError> {
    let state = State::new(initial.to_string());
    let current = state.clone();
    let changes = Rc::new(Cell::new(0));
    let count = changes.clone();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let current = current.clone();
        let count = count.clone();
        ui.text_area(
            current.get(),
            TextFieldOptions::default(),
            Modifier::new()
                .semantics(crate::accessibility::Semantics {
                    state: crate::accessibility::SemanticsState {
                        form: crate::accessibility::FormState {
                            read_only,
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                    ..Default::default()
                })
                .tag("field")
                .size(240.dp(), 60.dp()),
            move |value| {
                count.set(count.get() + 1);
                current.set(value);
            },
        );
    });
    advance_with(scene, &mut view, WINDOW, &mut Shaped)?;
    Ok((view, state, changes))
}

fn field_id(view: &View) -> Result<NodeId, TestError> {
    let field = (view.nodes.iter())
        .find(|node| node.tag.as_deref() == Some("field"))
        .required("a node tagged field")?;
    Ok(field.id)
}

/// Focuses the text area tagged "field" and selects `start..end` in it by setting its focus, anchor
/// and cursor directly, without input events.
pub(super) fn select_range(view: &mut View, start: usize, end: usize) -> TestResult {
    let id = field_id(view)?;
    view.focus = Some(id);
    let edit = view.edits.get_mut(&id).required("an edit of the field")?;
    edit.anchor = Some(start);
    edit.cursor = end;
    Ok(())
}

/// Does what `select_range` does and returns the node id of the text area.
pub(super) fn select(view: &mut View, start: usize, end: usize) -> Result<NodeId, TestError> {
    select_range(view, start, end)?;
    field_id(view)
}

pub(super) fn point(column: usize) -> Point {
    Point::new(repeated(10.dp(), column) + 1.dp(), 5.dp())
}

#[test]
fn selected_text_press_is_pending_until_threshold_and_click_collapses_on_release() -> TestResult {
    let mut scene = Scene::default();
    let (mut view, state, changes) = editor(&mut scene, "abc def", false)?;
    let id = select(&mut view, 0, 3)?;
    view.input(InputEvent::Press(point(1)));
    assert_eq!(view.edits[&id].selection(), 0..3);
    view.input(InputEvent::Move(Point::new(13.dp(), 5.dp())));
    assert!(view.edits[&id].drop_caret().is_none());
    assert_eq!(view.copy_selection(false).as_deref(), Some("abc"));
    view.input(InputEvent::Release(Point::new(13.dp(), 5.dp())));
    assert_eq!(view.edits[&id].selection(), 1..1);
    assert_eq!(state.get(), "abc def");
    assert_eq!(changes.get(), 0);
    Ok(())
}

#[test]
fn selected_text_moves_forward_and_backward_once_and_selects_the_result() -> TestResult {
    let mut scene = Scene::default();
    let (mut view, state, changes) = editor(&mut scene, "abc def", false)?;
    let id = select(&mut view, 0, 3)?;
    view.input(InputEvent::Press(point(1)));
    view.input(InputEvent::Move(point(7)));
    let caret = view.edits[&id].drop_caret().required("a drop caret")?;
    assert_eq!(caret.index, 7);
    assert_eq!(state.get(), "abc def");
    view.input(InputEvent::Release(point(7)));
    assert_eq!(state.get(), " defabc");
    assert_eq!(view.edits[&id].selection(), 4..7);
    assert_eq!(changes.get(), 1);
    advance_with(&mut scene, &mut view, WINDOW, &mut Shaped)?;
    view.input(InputEvent::Press(point(5)));
    view.input(InputEvent::Move(point(0)));
    view.input(InputEvent::Release(point(0)));
    assert_eq!(state.get(), "abc def");
    assert_eq!(view.edits[&id].selection(), 0..3);
    assert_eq!(changes.get(), 2);
    Ok(())
}

#[test]
fn same_range_drop_escape_outside_and_read_only_preserve_source() -> TestResult {
    for read_only in [false, true] {
        let mut scene = Scene::default();
        let (mut view, state, changes) = editor(&mut scene, "abc def", read_only)?;
        let id = select(&mut view, 0, 3)?;
        view.input(InputEvent::Press(point(1)));
        view.input(InputEvent::Move(point(2)));
        view.input(InputEvent::Release(point(2)));
        assert_eq!(view.edits[&id].selection(), 0..3);
        view.input(InputEvent::Press(point(1)));
        view.input(InputEvent::Move(point(7)));
        view.input(InputEvent::Key {
            key: Key::Escape,
            shift: false,
        });
        view.input(InputEvent::Release(point(7)));
        assert_eq!(view.edits[&id].selection(), 0..3);
        view.input(InputEvent::Press(point(1)));
        view.input(InputEvent::Move(Point::new(280.dp(), 5.dp())));
        view.input(InputEvent::Release(Point::new(280.dp(), 5.dp())));
        assert_eq!(view.edits[&id].selection(), 0..3);
        if read_only {
            view.input(InputEvent::Press(point(1)));
            view.input(InputEvent::Move(point(7)));
            view.input(InputEvent::Release(point(7)));
        }
        assert_eq!(state.get(), "abc def");
        assert_eq!(changes.get(), 0);
    }
    Ok(())
}

#[test]
fn duplicate_strings_use_captured_ranges_and_changed_contents_cancel_move() -> TestResult {
    let mut scene = Scene::default();
    let (mut view, state, changes) = editor(&mut scene, "abc abc xyz", false)?;
    select_range(&mut view, 4, 7)?;
    view.input(InputEvent::Press(point(5)));
    view.input(InputEvent::Move(point(11)));
    view.input(InputEvent::Release(point(11)));
    assert_eq!(state.get(), "abc  xyzabc");
    assert_eq!(changes.get(), 1);
    advance_with(&mut scene, &mut view, WINDOW, &mut Shaped)?;
    select_range(&mut view, 0, 3)?;
    view.input(InputEvent::Press(point(1)));
    view.input(InputEvent::Move(point(11)));
    state.set("changed elsewhere".to_string());
    advance_with(&mut scene, &mut view, WINDOW, &mut Shaped)?;
    view.input(InputEvent::Release(point(11)));
    assert_eq!(state.get(), "changed elsewhere");
    assert_eq!(changes.get(), 1);
    Ok(())
}

#[test]
fn combining_and_emoji_move_as_shaped_units() -> TestResult {
    let mut scene = Scene::default();
    let (mut view, state, changes) = editor(&mut scene, "e\u{301}👩\u{200d}💻z", false)?;
    let id = select(&mut view, 3, 14)?;
    view.input(InputEvent::Press(point(1)));
    view.input(InputEvent::Move(point(0)));
    view.input(InputEvent::Release(point(0)));
    assert_eq!(state.get(), "👩\u{200d}💻e\u{301}z");
    assert_eq!(view.edits[&id].selection(), 0..11);
    assert_eq!(changes.get(), 1);
    Ok(())
}

#[test]
fn plain_text_inherits_selection_copies_and_exposes_only_selection_actions() -> TestResult {
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), |ui| {
        ui.layout(Axis::Vertical, Modifier::new().user_select(true), |ui| {
            ui.text("read only text", Modifier::new().tag("field"));
            ui.text(
                "excluded",
                Modifier::new().user_select(false).tag("excluded"),
            );
        });
    });
    advance_with(&mut scene, &mut view, WINDOW, &mut Shaped)?;
    let id = select(&mut view, 0, 4)?;
    assert_eq!(view.copy_selection(false).as_deref(), Some("read"));
    assert_eq!(view.editing_target(), None);
    view.input(InputEvent::Shortcut(Shortcut::control(Key::Character('a'))));
    assert_eq!(
        view.copy_selection(false).as_deref(),
        Some("read only text")
    );
    view.input(InputEvent::Text("replacement".to_string()));
    assert_eq!(
        view.copy_selection(false).as_deref(),
        Some("read only text")
    );
    advance_with(&mut scene, &mut view, WINDOW, &mut Shaped)?;
    let node = &view.accessibility.nodes[&id];
    assert!(node.state.semantics.form.read_only);
    assert!(node.actions.contains(&ActionKind::SetTextSelection));
    assert!(!node.actions.contains(&ActionKind::SetValue));
    assert!(!node.actions.contains(&ActionKind::ReplaceSelectedText));
    assert_eq!(
        view.accessibility_action(ActionRequest {
            target: id,
            action: AccessibilityAction::SetTextSelection(TextSelection {
                anchor: 5,
                focus: 9
            }),
        }),
        Ok(ActionOutcome::Performed)
    );
    assert_eq!(view.copy_selection(false).as_deref(), Some("only"));
    let excluded = view
        .nodes
        .iter()
        .find(|node| node.tag.as_deref() == Some("excluded"))
        .required("a node tagged excluded")?
        .id;
    assert!(!view.hits.iter().any(|hit| hit.id == excluded));
    view.set_accessibility_text_geometry(true);
    view.set_accessibility_text_geometry(false);
    assert!(view.text_positions.contains_key(&id));
    Ok(())
}

#[test]
fn shaped_drop_snapshot_ignores_focus_and_rejects_changes_disabled_and_read_only() -> TestResult {
    let mut scene = Scene::default();
    let (mut view, state, changes) = editor(&mut scene, "abc def", false)?;
    let target = view.text_drop_target(point(3)).required("a drop target")?;
    view.focus = None;
    assert!(view.insert_text_at(&target, "\r\nx"));
    assert_eq!(state.get(), "abc\nx def");
    assert_eq!(changes.get(), 1);
    advance_with(&mut scene, &mut view, WINDOW, &mut Shaped)?;
    let target = view.text_drop_target(point(0)).required("a drop target")?;
    state.set("new contents".to_string());
    assert!(!view.insert_text_at(&target, "stale"));
    advance_with(&mut scene, &mut view, WINDOW, &mut Shaped)?;
    assert!(!view.insert_text_at(&target, "stale"));
    let index = view
        .nodes
        .iter()
        .position(|node| node.tag.as_deref() == Some("field"))
        .required("a node tagged field")?;
    let target = view.text_drop_target(point(0)).required("a drop target")?;
    view.nodes[index].enabled = false;
    assert!(!view.insert_text_at(&target, "disabled"));
    let mut read_only_scene = Scene::default();
    let (view, _, _) = editor(&mut read_only_scene, "abc", true)?;
    assert!(view.text_drop_target(point(0)).is_none());
    Ok(())
}

#[test]
fn pointer_selection_of_centered_plain_text_repaints_retained_geometry() -> TestResult {
    let mut view = View::new(crate::platform::fixture::facts(), |ui| {
        ui.provide(
            &crate::style::INTERACTION_STYLE,
            crate::style::InteractionStyle {
                selection: Some(Color::WHITE),
                ..Default::default()
            },
            |ui| {
                ui.scoped("paragraph", Modifier::new().user_select(true), |ui| {
                    ui.text(
                        "abc def",
                        Modifier::new().tag("field").size(140.dp(), 80.dp()),
                    );
                });
            },
        );
    });
    let window = Viewport::new(Size::new(300.dp(), 100.dp()), Density::ONE);
    let mut scene = Scene::default();
    scene.show(view.frame(window, &mut Shaped)?)?;
    let before = view.stats();
    view.input(InputEvent::Press(Point::new(1.dp(), 35.dp())));
    view.input(InputEvent::Move(Point::new(31.dp(), 35.dp())));
    view.input(InputEvent::Release(Point::new(31.dp(), 35.dp())));
    assert_eq!(view.copy_selection(false).as_deref(), Some("abc"));
    scene.show(view.frame(window, &mut Shaped)?)?;
    assert_eq!(view.stats().root_compositions, before.root_compositions);
    assert!(view.stats().painted_components > before.painted_components);
    assert!(scene.commands().iter().any(|command| matches!(command,
        DrawCommand::Fill { shape, .. } if *shape == Rect::new(0.dp(), 30.dp(), 30.dp(), 20.dp()).into())));
    let id = view.focus.required("a focused node")?;
    assert_eq!(
        view.accessibility.nodes[&id]
            .text
            .as_ref()
            .required("the text of the editor")?
            .selection,
        Some(TextSelection {
            anchor: 0,
            focus: 3
        })
    );
    let glyph = view.accessibility.nodes[&id]
        .text
        .as_ref()
        .required("the text of the node")?
        .runs[0]
        .character_bounds[0];
    assert_eq!(glyph.y, 30.dp());
    Ok(())
}

#[test]
fn local_drag_release_rejects_newer_application_text_before_composition() -> TestResult {
    let mut scene = Scene::default();
    let (mut view, state, changes) = editor(&mut scene, "abc def", false)?;
    select_range(&mut view, 0, 3)?;
    view.input(InputEvent::Press(point(1)));
    view.input(InputEvent::Move(point(7)));
    state.set("external update".to_string());
    view.input(InputEvent::Release(point(7)));
    assert_eq!(state.get(), "external update");
    assert_eq!(changes.get(), 0);
    Ok(())
}

#[test]
fn disabled_drag_source_clears_its_insertion_marker_on_release() -> TestResult {
    let enabled = State::new(true);
    let availability = enabled.clone();
    let state = State::new("abc def".to_string());
    let current = state.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let current = current.clone();
        ui.text_area(
            current.get(),
            TextFieldOptions::default(),
            Modifier::new()
                .enabled(availability.get())
                .tag("field")
                .size(240.dp(), 60.dp()),
            move |value| current.set(value),
        );
    });
    let window = Viewport::new(Size::new(300.dp(), 100.dp()), Density::ONE);
    advance_with(&mut scene, &mut view, window, &mut Shaped)?;
    let id = select(&mut view, 0, 3)?;
    view.input(InputEvent::Press(point(1)));
    view.input(InputEvent::Move(point(7)));
    assert!(view.edits[&id].drop_caret().is_some());
    enabled.set(false);
    advance_with(&mut scene, &mut view, window, &mut Shaped)?;
    assert!(!view.hits.iter().any(|hit| hit.id == id));
    view.input(InputEvent::Release(point(7)));
    assert!(view.edits[&id].drop_caret().is_none());
    assert_eq!(state.get(), "abc def");
    Ok(())
}
