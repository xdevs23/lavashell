// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::accessibility::{
    AccessibilityAction, ActionError, ActionOutcome, ActionRequest, FormState, IgnoredBecause,
    Relations, Role, SemanticsMode, TextSelection,
};
use crate::geometry::{Axis, Point, Size, Viewport};
use crate::keyboard::{Key, Shortcut};
use crate::scene::TextStyle;
use crate::test_error::{Required, TestError, TestResult};
use crate::ui::{Modifier, NodeId, TextFieldOptions};
use crate::units::{Density, Dp, DpExt};
use crate::view::press_and_release_at;
use crate::view::test_scene::{Scene, advance_with};
use crate::view::test_tags::Tagged;
use crate::view::test_text::repeated;
use crate::view::{InputEvent, TextMeasurer};

mod actions;
mod dynamic;
mod focus;
mod text;

struct Metrics;

impl TextMeasurer for Metrics {
    fn measure(&mut self, value: &str, style: &TextStyle<Dp>, width: Dp) -> Size {
        let natural = repeated(8.dp(), value.chars().count());
        Size::new(natural.min(width), style.size)
    }
}

fn frame(scene: &mut Scene, view: &mut View) -> TestResult {
    advance_with(
        scene,
        view,
        Viewport::new(Size::new(300.dp(), 200.dp()), Density::ONE),
        &mut Metrics,
    )
}

fn target(view: &View, name: &str) -> Result<NodeId, TestError> {
    let node = (view.accessibility.tree.nodes.values())
        .find(|node| node.name.as_deref() == Some(name))
        .required("an accessibility node with that name")?;
    Ok(node.id)
}

const PERFORMED: Result<ActionOutcome, ActionError> = Ok(ActionOutcome::Performed);

fn request(
    view: &mut View,
    target: NodeId,
    action: AccessibilityAction,
) -> Result<ActionOutcome, ActionError> {
    view.accessibility_action(ActionRequest { target, action })
}

#[test]
fn passive_button_text_merges_and_actions_share_callback_without_recomposition() -> TestResult {
    let activations = crate::reactive::State::new(0);
    let called = activations.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let called = called.clone();
        ui.layout(
            Axis::Horizontal,
            Modifier::new().on_activate(move |_| called.set(called.get() + 1)),
            |ui| {
                ui.text("Run", Modifier::new());
            },
        );
    });
    frame(&mut scene, &mut view)?;
    let button = target(&view, "Run")?;
    let node = &view.accessibility.tree.nodes[&button];
    assert_eq!(node.role, Role::Button);
    assert_eq!(node.children, []);
    assert_eq!(view.accessibility.tree.nodes.len(), 2);
    let before = view.stats().root_compositions;
    assert_eq!(
        request(&mut view, button, AccessibilityAction::Activate),
        PERFORMED
    );
    assert_eq!(activations.get(), 1);
    assert_eq!(
        request(&mut view, button, AccessibilityAction::Focus),
        PERFORMED
    );
    assert_eq!(view.focus(), Some(button));
    frame(&mut scene, &mut view)?;
    assert_eq!(view.stats().root_compositions, before);
    let previous = view.accessibility.tree.clone();
    frame(&mut scene, &mut view)?;
    assert_eq!(view.accessibility.tree.revision, previous.revision);
    assert_eq!(view.accessibility.tree.nodes, previous.nodes);
    Ok(())
}

#[test]
fn a_keyed_node_has_one_identity_through_an_update_and_leaves_on_removal() -> TestResult {
    let visible = crate::reactive::State::new(true);
    let text = crate::reactive::State::new(String::from("First"));
    let show = visible.clone();
    let label = text.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        if show.get() {
            ui.key("stable", Modifier::new(), |ui| {
                ui.text(label.get(), Modifier::new());
            });
        }
    });
    frame(&mut scene, &mut view)?;
    let id = target(&view, "First")?;
    text.set(String::from("Second"));
    frame(&mut scene, &mut view)?;
    assert_eq!(target(&view, "Second")?, id);
    visible.set(false);
    frame(&mut scene, &mut view)?;
    assert!(!view.accessibility.tree.nodes.contains_key(&id));
    assert_eq!(
        request(&mut view, id, AccessibilityAction::Activate),
        Ok(ActionOutcome::Ignored(IgnoredBecause::UnknownTarget))
    );
    Ok(())
}

#[test]
fn disabled_inheritance_and_modal_filtering_reject_requests() -> TestResult {
    let modal = crate::reactive::State::new(false);
    let show = modal.clone();
    let activations = crate::reactive::State::new(0);
    let calls = activations.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let calls = calls.clone();
        ui.layout(Axis::Vertical, Modifier::new().enabled(false), |ui| {
            ui.text(
                "Disabled",
                Modifier::new().on_activate(move |_| calls.set(calls.get() + 1)),
            );
        });
        ui.text("Outside", Modifier::new().on_activate(|_| {}));
        if show.get() {
            ui.layout(Axis::Vertical, Modifier::new().modal(), |ui| {
                ui.text("Inside", Modifier::new().on_activate(|_| {}));
            });
        }
    });
    frame(&mut scene, &mut view)?;
    let disabled = target(&view, "Disabled")?;
    let outside = target(&view, "Outside")?;
    assert!(
        view.accessibility.tree.nodes[&disabled]
            .state
            .semantics
            .disabled
    );
    assert!(view.accessibility.tree.nodes[&disabled].actions.is_empty());
    assert_eq!(
        request(&mut view, disabled, AccessibilityAction::Activate),
        Ok(ActionOutcome::Ignored(IgnoredBecause::Disabled))
    );
    assert_eq!(activations.get(), 0);
    modal.set(true);
    frame(&mut scene, &mut view)?;
    assert!(!view.accessibility.tree.nodes.contains_key(&outside));
    assert_eq!(
        request(&mut view, outside, AccessibilityAction::Activate),
        Ok(ActionOutcome::Ignored(IgnoredBecause::UnknownTarget))
    );
    assert!(
        view.accessibility
            .tree
            .nodes
            .contains_key(&target(&view, "Inside")?)
    );
    assert_eq!(view.accessibility.tree.nodes[&NodeId(1)].children.len(), 1);
    Ok(())
}

#[test]
fn decorative_hidden_and_clear_have_distinct_meanings() -> TestResult {
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), |ui| {
        ui.layout(
            Axis::Vertical,
            Modifier::new().semantics(Semantics {
                mode: SemanticsMode::Decorative,
                ..Default::default()
            }),
            |ui| {
                ui.text("Promoted", Modifier::new());
            },
        );
        ui.layout(
            Axis::Vertical,
            Modifier::new().semantics(Semantics {
                mode: SemanticsMode::Hidden,
                ..Default::default()
            }),
            |ui| {
                ui.text("Secret", Modifier::new());
            },
        );
        ui.layout(
            Axis::Vertical,
            Modifier::new().semantics(Semantics {
                mode: SemanticsMode::ClearDescendants,
                name: Some(String::from("Summary")),
                ..Default::default()
            }),
            |ui| {
                ui.text("Replaced", Modifier::new().on_activate(|_| {}));
            },
        );
    });
    frame(&mut scene, &mut view)?;
    assert!(
        view.accessibility
            .tree
            .nodes
            .values()
            .any(|node| node.name.as_deref() == Some("Promoted"))
    );
    assert!(
        !view
            .accessibility
            .tree
            .nodes
            .values()
            .any(|node| node.name.as_deref() == Some("Secret"))
    );
    assert!(
        !view
            .accessibility
            .tree
            .nodes
            .values()
            .any(|node| node.name.as_deref() == Some("Replaced"))
    );
    assert_eq!(
        view.accessibility.tree.nodes[&target(&view, "Summary")?].children,
        []
    );
    Ok(())
}

#[test]
fn unicode_selection_and_replacement_preserve_directed_scalar_offsets() -> TestResult {
    let value = crate::reactive::State::new(String::from("aé🦀z"));
    let content = value.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let content = content.clone();
        ui.text_input(
            content.get(),
            TextFieldOptions::default(),
            Modifier::new().semantics(Semantics {
                name: Some(String::from("Editor")),
                ..Default::default()
            }),
            move |value| content.set(value),
        );
    });
    frame(&mut scene, &mut view)?;
    let editor = target(&view, "Editor")?;
    assert_eq!(
        request(
            &mut view,
            editor,
            AccessibilityAction::SetTextSelection(TextSelection {
                anchor: 3,
                focus: 1
            })
        ),
        PERFORMED
    );
    assert_eq!(
        view.accessibility.tree.nodes[&editor]
            .text
            .as_ref()
            .required("the text of the editor")?
            .selection,
        Some(TextSelection {
            anchor: 3,
            focus: 1
        })
    );
    assert_eq!(
        request(
            &mut view,
            editor,
            AccessibilityAction::SetTextSelection(TextSelection {
                anchor: 9,
                focus: 0
            })
        ),
        Ok(ActionOutcome::Ignored(IgnoredBecause::SelectionOutsideText))
    );
    assert_eq!(
        request(
            &mut view,
            editor,
            AccessibilityAction::ReplaceSelectedText(String::from("🚲"))
        ),
        PERFORMED
    );
    assert_eq!(value.get(), "a🚲z");
    assert_eq!(view.focus(), None);
    Ok(())
}

#[test]
fn snapshots_can_cross_threads() {
    fn require_send_sync<T: Send + Sync>() {}
    require_send_sync::<AccessibilityTree>();
    require_send_sync::<ActionRequest>();
}

#[test]
fn modal_filters_later_painted_controls_and_direct_wheel_paths() -> TestResult {
    let scroll = crate::scroll::ScrollState::new(Axis::Vertical);
    let state = scroll.clone();
    let activations = crate::reactive::State::new(0);
    let called = activations.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        ui.scroll(state.clone(), Modifier::new().height(80.dp()), |ui| {
            for _ in 0..20 {
                ui.text("Background", Modifier::new());
            }
        });
        ui.layout(
            Axis::Vertical,
            Modifier::new().modal().height(30.dp()),
            |ui| {
                ui.text("Dialog", Modifier::new());
            },
        );
        let called = called.clone();
        ui.text(
            "Later",
            Modifier::new()
                .tag("late")
                .on_activate(move |_| called.set(called.get() + 1)),
        );
    });
    frame(&mut scene, &mut view)?;
    let later = view.tagged_bounds(&"late")?;
    let point = Point::new(later.x + 1.dp(), later.y + 1.dp());
    press_and_release_at(&mut view, point);
    assert_eq!(activations.get(), 0);
    view.input(InputEvent::Scroll {
        at: Point::new(10.dp(), 60.dp()),
        delta: Point::new(Dp::ZERO, 40.dp()),
    });
    assert_eq!(scroll.offset(), Dp::ZERO);
    assert!(
        !view
            .accessibility
            .tree
            .nodes
            .values()
            .any(|node| node.name.as_deref() == Some("Later"))
    );
    Ok(())
}

#[test]
fn tags_never_become_spoken_names_and_relations_use_existing_text() -> TestResult {
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), |ui| {
        let label = ui.next_id();
        ui.text("Existing field copy", Modifier::new().tag("debug-label"));
        ui.text_input(
            "",
            TextFieldOptions {
                placeholder: "Existing hint".into(),
                ..TextFieldOptions::default()
            },
            Modifier::new().tag("debug-field").semantics(Semantics {
                relations: Relations {
                    labelled_by: vec![label],
                    ..Default::default()
                },
                ..Default::default()
            }),
            |_| {},
        );
        ui.text(
            "Existing button copy",
            Modifier::new().tag("debug-button").on_activate(|_| {}),
        );
    });
    frame(&mut scene, &mut view)?;
    assert!(!view.accessibility.tree.nodes.values().any(|node| {
        node.name
            .as_ref()
            .is_some_and(|name| name.starts_with("debug"))
    }));
    let field = view
        .accessibility
        .tree
        .nodes
        .values()
        .find(|node| node.role == Role::TextInput)
        .required("a text input node")?;
    assert_eq!(field.name.as_deref(), Some("Existing field copy"));
    assert_eq!(
        field
            .text
            .as_ref()
            .required("the text of the text input node")?
            .placeholder
            .as_deref(),
        Some("Existing hint")
    );
    assert_eq!(
        view.accessibility.tree.nodes[&target(&view, "Existing button copy")?].role,
        Role::Button
    );
    Ok(())
}
