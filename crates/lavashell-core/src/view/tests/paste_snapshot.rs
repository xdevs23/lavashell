// SPDX-License-Identifier: GPL-3.0-only
use super::selection::{Shaped, editor, point, select, select_range};
use super::*;

const WINDOW: Viewport = Viewport::new(Size::new(Dp::new(300.0), Dp::new(100.0)), Density::ONE);

#[test]
fn paste_replaces_captured_selection_independently_of_focus_and_current_selection() -> TestResult {
    let mut scene = Scene::default();
    let (mut view, state, changes) = editor(&mut scene, "abc abc xyz", false)?;
    let id = select(&mut view, 4, 7)?;
    let target = view.text_paste_target(id).required("a paste target")?;
    select_range(&mut view, 0, 3)?;
    view.focus = None;
    assert!(view.paste_text_at(&target, "snow\r\n雪"));
    assert_eq!(state.get(), "abc snow\n雪 xyz");
    assert_eq!(changes.get(), 1);
    Ok(())
}

#[test]
fn paste_uses_shaped_emoji_range_and_empty_caret_selection() -> TestResult {
    let mut scene = Scene::default();
    let (mut view, state, changes) = editor(&mut scene, "e\u{301}👩\u{200d}💻z", false)?;
    let id = select(&mut view, 3, 14)?;
    let target = view.text_paste_target(id).required("a paste target")?;
    assert!(view.paste_text_at(&target, "🦀"));
    assert_eq!(state.get(), "e\u{301}🦀z");
    assert_eq!(changes.get(), 1);
    let mut second_scene = Scene::default();
    let (mut view, state, _) = editor(&mut second_scene, "abc", false)?;
    let id = select(&mut view, 1, 1)?;
    let target = view.text_paste_target(id).required("a paste target")?;
    assert!(view.paste_text_at(&target, "X"));
    assert_eq!(state.get(), "aXbc");
    Ok(())
}

#[test]
fn changed_text_disabled_and_read_only_paste_targets_fail_without_editing() -> TestResult {
    let mut scene = Scene::default();
    let (mut view, state, changes) = editor(&mut scene, "abc", false)?;
    let id = select(&mut view, 0, 3)?;
    let target = view.text_paste_target(id).required("a paste target")?;
    state.set("external text".to_string());
    assert!(!view.paste_text_at(&target, "stale"));
    advance_with(&mut scene, &mut view, WINDOW, &mut Shaped)?;
    assert!(!view.paste_text_at(&target, "stale"));
    assert_eq!(state.get(), "external text");
    assert_eq!(changes.get(), 0);
    let target = view.text_paste_target(id).required("a paste target")?;
    view.nodes
        .iter_mut()
        .find(|node| node.id == id)
        .required("the node of the field")?
        .enabled = false;
    assert!(!view.paste_text_at(&target, "disabled"));
    let mut read_only_scene = Scene::default();
    let (view, _, _) = editor(&mut read_only_scene, "abc", true)?;
    assert!(view.text_paste_target(view.nodes[1].id).is_none());
    Ok(())
}

#[test]
fn removed_or_modal_excluded_paste_target_cannot_be_redirected() -> TestResult {
    for remove in [false, true] {
        let hidden = State::new(false);
        let condition = hidden.clone();
        let text = State::new("abc".to_string());
        let current = text.clone();
        let mut scene = Scene::default();
        let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
            ui.key("source", Modifier::new(), |ui| {
                if !(remove && condition.get()) {
                    let current = current.clone();
                    ui.text_area(
                        current.get(),
                        TextFieldOptions::default(),
                        Modifier::new().tag("field"),
                        move |value| {
                            current.set(value);
                        },
                    );
                }
            });
            if condition.get() && !remove {
                ui.layout(Arrangement::Overlay, Modifier::new().modal(), |ui| {
                    ui.text("modal", Modifier::new());
                });
            }
        });
        let window = WINDOW;
        advance_with(&mut scene, &mut view, window, &mut Shaped)?;
        let id = select(&mut view, 0, 3)?;
        let target = view.text_paste_target(id).required("a paste target")?;
        hidden.set(true);
        advance_with(&mut scene, &mut view, window, &mut Shaped)?;
        assert!(!view.paste_text_at(&target, "wrong field"));
        assert_eq!(text.get(), "abc");
    }
    Ok(())
}

#[test]
fn native_text_drag_query_shares_local_threshold_and_field_bounds() -> TestResult {
    let mut scene = Scene::default();
    let (mut view, _, _) = editor(&mut scene, "abc def", false)?;
    let id = select(&mut view, 0, 3)?;
    view.input(InputEvent::Press(point(1)));
    let source = view
        .text_selection_source_at(point(1))
        .required("a selection source")?;
    assert_eq!(source.id(), id);
    assert!(view.text_source_contains(&source, point(7)));
    assert!(!view.text_source_contains(&source, Point::new(250.dp(), 5.dp())));
    assert!(!view.text_source_drag_ready(&source, Point::new(13.dp(), 5.dp())));
    assert!(view.text_source_drag_ready(&source, Point::new(250.dp(), 5.dp())));
    Ok(())
}
