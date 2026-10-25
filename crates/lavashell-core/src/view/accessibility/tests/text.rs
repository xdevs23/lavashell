// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::view::{TextCaret, TextCluster};
use std::{cell::Cell, rc::Rc};

const WINDOW: Viewport = Viewport::new(Size::new(Dp::new(300.0), Dp::new(200.0)), Density::ONE);

struct ClusterMetrics;

impl TextMeasurer for ClusterMetrics {
    fn measure(&mut self, _: &str, _: &TextStyle<Dp>, width: Dp) -> Size {
        Size::new(60.dp().min(width), 20.dp())
    }

    fn geometry(&mut self, value: &str, _: &TextStyle<Dp>, width: Dp) -> TextGeometry {
        assert_eq!(value, "e\u{301}👩\u{200d}💻z");
        TextGeometry {
            carets: [0, 3, 14, 15]
                .into_iter()
                .enumerate()
                .map(|(column, index)| TextCaret {
                    index,
                    rect: Rect::new(repeated(20.dp(), column), 0.dp(), 1.dp(), 20.dp()),
                    line: 0,
                })
                .collect(),
            size: Size::new(width, 20.dp()),
            ..Default::default()
        }
    }
}

#[test]
fn shaped_clusters_define_selectable_units_and_reject_interior_scalar_selection() -> TestResult {
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), |ui| {
        ui.text_input(
            "e\u{301}👩\u{200d}💻z",
            TextFieldOptions::default(),
            Modifier::new()
                .semantics(Semantics {
                    name: Some(String::from("Cluster editor")),
                    ..Default::default()
                })
                .height(24.dp()),
            |_| {},
        );
    });
    advance_with(&mut scene, &mut view, WINDOW, &mut ClusterMetrics)?;
    let editor = target(&view, "Cluster editor")?;
    let text = view.accessibility.nodes[&editor]
        .text
        .as_ref()
        .required("the text of the editor")?;
    assert_eq!(text.runs[0].selectable_lengths, [2, 3, 1]);
    assert_eq!(text.runs[0].character_bounds.len(), 6);
    assert_eq!(text.runs[0].direction, TextDirection::LeftToRight);
    assert_eq!(text.runs[0].word_starts, None);
    assert_eq!(
        request(
            &mut view,
            editor,
            AccessibilityAction::SetTextSelection(TextSelection {
                anchor: 0,
                focus: 1
            })
        ),
        IGNORED
    );
    assert_eq!(
        request(
            &mut view,
            editor,
            AccessibilityAction::SetTextSelection(TextSelection {
                anchor: 2,
                focus: 3
            })
        ),
        IGNORED
    );
    assert_eq!(
        request(
            &mut view,
            editor,
            AccessibilityAction::SetTextSelection(TextSelection {
                anchor: 5,
                focus: 2
            })
        ),
        PERFORMED
    );
    assert_eq!(
        view.accessibility.nodes[&editor]
            .text
            .as_ref()
            .required("the text of the editor")?
            .selection,
        Some(TextSelection {
            anchor: 5,
            focus: 2
        })
    );
    let glyph = view.accessibility.nodes[&editor]
        .text
        .as_ref()
        .required("the text of the editor")?
        .runs[0]
        .character_bounds[0];
    assert_eq!(
        glyph.y,
        2.dp(),
        "single-line glyph and keyboard caret center within the 24px field"
    );
    Ok(())
}

#[test]
fn read_only_fields_keep_navigation_and_copy_but_reject_all_edits() -> TestResult {
    let value = crate::reactive::State::new(String::from("aé🦀z"));
    let current = value.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let changed = current.clone();
        ui.text_input(
            current.get(),
            TextFieldOptions::default(),
            Modifier::new().tag("Read only").semantics(Semantics {
                name: Some(String::from("Read only")),
                state: SemanticsState {
                    form: FormState {
                        read_only: true,
                        ..Default::default()
                    },
                    ..Default::default()
                },
                ..Default::default()
            }),
            move |value| changed.set(value),
        );
    });
    frame(&mut scene, &mut view)?;
    let editor = target(&view, "Read only")?;
    assert_eq!(
        request(&mut view, editor, AccessibilityAction::Focus),
        PERFORMED
    );
    view.input(InputEvent::Shortcut(Shortcut::control(Key::Character('a'))));
    assert_eq!(view.copy_selection(false).as_deref(), Some("aé🦀z"));
    view.input(InputEvent::Text(String::from("replacement")));
    view.input(InputEvent::Key {
        key: Key::Backspace,
        shift: false,
    });
    view.input(InputEvent::Key {
        key: Key::Delete,
        shift: false,
    });
    assert!(view.text_paste_target(editor).is_none());
    assert_eq!(view.copy_selection(true).as_deref(), Some("aé🦀z"));
    assert_eq!(value.get(), "aé🦀z");
    assert_eq!(
        request(
            &mut view,
            editor,
            AccessibilityAction::SetValue(AccessibleValue::Text(String::from("edit")))
        ),
        IGNORED
    );
    assert_eq!(
        request(
            &mut view,
            editor,
            AccessibilityAction::ReplaceSelectedText(String::from("edit"))
        ),
        IGNORED
    );
    assert_eq!(
        request(
            &mut view,
            editor,
            AccessibilityAction::SetTextSelection(TextSelection {
                anchor: 1,
                focus: 3
            })
        ),
        PERFORMED
    );
    view.input(InputEvent::Key {
        key: Key::Left,
        shift: false,
    });
    frame(&mut scene, &mut view)?;
    assert_eq!(
        view.accessibility.nodes[&editor]
            .text
            .as_ref()
            .required("the text of the editor")?
            .selection,
        Some(TextSelection {
            anchor: 1,
            focus: 1
        })
    );
    Ok(())
}

struct CountGeometry(Rc<Cell<usize>>);

impl TextMeasurer for CountGeometry {
    fn measure(&mut self, _: &str, _: &TextStyle<Dp>, width: Dp) -> Size {
        Size::new(30.dp().min(width), 20.dp())
    }

    fn geometry(&mut self, value: &str, _: &TextStyle<Dp>, width: Dp) -> TextGeometry {
        self.0.set(self.0.get() + 1);
        TextGeometry {
            carets: value
                .char_indices()
                .map(|(index, _)| index)
                .chain([value.len()])
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

#[test]
fn static_geometry_is_lazy_retained_and_uses_visible_text_origin() -> TestResult {
    let shapes = Rc::new(Cell::new(0));
    let mut metrics = CountGeometry(shapes.clone());
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), |ui| {
        ui.scoped("label", Modifier::new(), |ui| {
            ui.text(
                "abc",
                Modifier::new()
                    .size(100.dp(), 40.dp())
                    .offset(10.dp(), 8.dp()),
            );
        });
    });
    advance_with(&mut scene, &mut view, WINDOW, &mut metrics)?;
    assert_eq!(shapes.get(), 0);
    let compositions = view.stats().root_compositions;
    view.set_accessibility_text_geometry(true);
    advance_with(&mut scene, &mut view, WINDOW, &mut metrics)?;
    assert_eq!(shapes.get(), 1);
    let id = target(&view, "abc")?;
    let glyph = view.accessibility.nodes[&id]
        .text
        .as_ref()
        .required("the text of the node")?
        .runs[0]
        .character_bounds[0];
    assert_eq!(glyph, Rect::new(10.dp(), 18.dp(), 10.dp(), 20.dp()));
    let previous = view.accessibility.clone();
    advance_with(&mut scene, &mut view, WINDOW, &mut metrics)?;
    assert_eq!(shapes.get(), 1);
    assert_eq!(view.accessibility, previous);
    assert_eq!(view.stats().root_compositions, compositions);
    view.set_accessibility_text_geometry(false);
    advance_with(&mut scene, &mut view, WINDOW, &mut metrics)?;
    assert_eq!(shapes.get(), 1);
    assert!(
        view.accessibility.nodes[&id]
            .text
            .as_ref()
            .required("the text of the node")?
            .runs
            .is_empty()
    );
    Ok(())
}

struct Bidirectional;

impl TextMeasurer for Bidirectional {
    fn measure(&mut self, _: &str, _: &TextStyle<Dp>, _: Dp) -> Size {
        Size::new(60.dp(), 20.dp())
    }

    fn geometry(&mut self, value: &str, _: &TextStyle<Dp>, width: Dp) -> TextGeometry {
        assert_eq!(value, "abcאבג");
        let positions = [
            (0, 0.dp()),
            (1, 10.dp()),
            (2, 20.dp()),
            (3, 60.dp()),
            (5, 50.dp()),
            (7, 40.dp()),
            (9, 30.dp()),
        ];
        TextGeometry {
            carets: positions
                .into_iter()
                .map(|(index, x)| TextCaret {
                    index,
                    rect: Rect::new(x, 0.dp(), 1.dp(), 20.dp()),
                    line: 0,
                })
                .collect(),
            size: Size::new(width, 20.dp()),
            lines: std::iter::once(0..value.len()).collect(),
            word_starts: Some(vec![0]),
            clusters: vec![
                TextCluster {
                    byte_range: 0..1,
                    bounds: Rect::new(0.dp(), 0.dp(), 10.dp(), 20.dp()),
                    direction: TextDirection::LeftToRight,
                    line: 0,
                },
                TextCluster {
                    byte_range: 1..2,
                    bounds: Rect::new(10.dp(), 0.dp(), 10.dp(), 20.dp()),
                    direction: TextDirection::LeftToRight,
                    line: 0,
                },
                TextCluster {
                    byte_range: 2..3,
                    bounds: Rect::new(20.dp(), 0.dp(), 10.dp(), 20.dp()),
                    direction: TextDirection::LeftToRight,
                    line: 0,
                },
                TextCluster {
                    byte_range: 3..5,
                    bounds: Rect::new(50.dp(), 0.dp(), 10.dp(), 20.dp()),
                    direction: TextDirection::RightToLeft,
                    line: 0,
                },
                TextCluster {
                    byte_range: 5..7,
                    bounds: Rect::new(40.dp(), 0.dp(), 10.dp(), 20.dp()),
                    direction: TextDirection::RightToLeft,
                    line: 0,
                },
                TextCluster {
                    byte_range: 7..9,
                    bounds: Rect::new(30.dp(), 0.dp(), 10.dp(), 20.dp()),
                    direction: TextDirection::RightToLeft,
                    line: 0,
                },
            ],
        }
    }
}

#[test]
fn mixed_bidirectional_text_splits_runs_and_preserves_logical_scalar_order() -> TestResult {
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), |ui| {
        ui.text("abcאבג", Modifier::new());
    });
    view.set_accessibility_text_geometry(true);
    advance_with(&mut scene, &mut view, WINDOW, &mut Bidirectional)?;
    let id = target(&view, "abcאבג")?;
    let runs = &view.accessibility.nodes[&id]
        .text
        .as_ref()
        .required("the text of the node")?
        .runs;
    assert_eq!(runs.len(), 2);
    assert_eq!(runs[0].text, "abc");
    assert_eq!(runs[0].line, 0);
    assert_eq!(runs[0].word_starts.as_deref(), Some(&[0][..]));
    assert_eq!(runs[0].direction, TextDirection::LeftToRight);
    assert_eq!(
        runs[0].character_bounds[2].width,
        10.dp(),
        "LTR extent must not span the adjoining RTL run"
    );
    assert_eq!(runs[1].text, "אבג");
    assert_eq!(runs[1].line, 0);
    assert_eq!(runs[1].word_starts.as_deref(), Some(&[][..]));
    assert_eq!(runs[1].start, 3);
    assert_eq!(runs[1].direction, TextDirection::RightToLeft);
    assert!(runs[1].character_bounds[0].x > runs[1].character_bounds[1].x);
    Ok(())
}
