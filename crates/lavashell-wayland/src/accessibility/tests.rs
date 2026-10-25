// SPDX-License-Identifier: GPL-3.0-only
mod native;
mod protected;
mod text;

use super::*;
use crate::test_error::TestError;
use accesskit::{Action, ActionData, NodeId as NativeId, Role as NativeRole, TreeId};
use lavashell_core::accessibility::*;
use lavashell_core::prelude::{Axis, DpExt, NodeId, Rect};
use std::collections::{BTreeMap, BTreeSet};

fn node(id: u64, role: Role, name: &str) -> AccessibleNode {
    AccessibleNode {
        id: NodeId(id),
        role,
        heading_level: None,
        name: Some(name.into()),
        description: None,
        value: None,
        bounds: Rect::new(10.dp(), 20.dp(), 100.dp(), 30.dp()),
        visible_bounds: Some(Rect::new(10.dp(), 20.dp(), 100.dp(), 30.dp())),
        children: Vec::new(),
        state: AccessibleState::default(),
        actions: ActionKinds::default(),
        relations: Relations::default(),
        live: LiveRegion::Off,
        range: None,
        collection: None,
        item: None,
        text: None,
    }
}

fn tree() -> AccessibilityTree {
    let mut root = node(u64::MAX, Role::Window, "Window");
    root.children = vec![NodeId(1), NodeId(1 << 63)];
    let mut button = node(1, Role::Button, "Start");
    button.actions = [ActionKind::Activate, ActionKind::Focus].into();
    button.state.focusable = true;
    let mut text = node(1 << 63, Role::TextInput, "Text");
    text.actions = [
        ActionKind::SetTextSelection,
        ActionKind::ReplaceSelectedText,
    ]
    .into();
    text.text = Some(AccessibleText {
        value: "a💡é".into(),
        selection: Some(TextSelection {
            anchor: 3,
            focus: 1,
        }),
        ..AccessibleText::default()
    });
    AccessibilityTree {
        revision: 1,
        root: root.id,
        focus: Some(button.id),
        nodes: [(root.id, root), (button.id, button), (text.id, text)].into(),
    }
}

fn node_mut(tree: &mut AccessibilityTree, id: NodeId) -> Result<&mut AccessibleNode, TestError> {
    tree.nodes.get_mut(&id).ok_or(TestError::MissingNode(id))
}

fn root_mut(tree: &mut AccessibilityTree) -> Result<&mut AccessibleNode, TestError> {
    let root = tree.root;
    node_mut(tree, root)
}

fn text_mut(tree: &mut AccessibilityTree, id: NodeId) -> Result<&mut AccessibleText, TestError> {
    node_mut(tree, id)?
        .text
        .as_mut()
        .ok_or(TestError::MissingText(id))
}

/// The first node of `update` with `role`.
fn native_node(
    update: &TreeUpdate,
    role: NativeRole,
) -> Result<&(NativeId, accesskit::Node), TestError> {
    (update.nodes.iter())
        .find(|(_, node)| node.role() == role)
        .ok_or(TestError::MissingNativeRole(role))
}

fn request(action: Action, node: NativeId, data: Option<ActionData>) -> accesskit::ActionRequest {
    accesskit::ActionRequest {
        action,
        target_tree: TreeId::ROOT,
        target_node: node,
        data,
    }
}

#[test]
fn native_ids_do_not_alias_full_width_core_or_virtual_nodes() {
    let tree = tree();
    let mut ids = ids::Ids::default();
    let native = mapping::tree(&tree, &mut ids, "fallback");
    let all = native
        .nodes
        .iter()
        .map(|(id, _)| *id)
        .collect::<BTreeSet<_>>();
    assert_eq!(all.len(), 4);
    assert_eq!(native.focus, ids.node(NodeId(1)));
    let again = mapping::tree(&tree, &mut ids, "fallback");
    assert_eq!(native, again);
}

/// A tree with a scroll view under the root that scrolls along `axis` and contains one item 12 dp
/// tall, the ids of its native nodes, and the native id and node of the scroll view.
fn scroll_view(
    axis: Axis,
) -> Result<(AccessibilityTree, ids::Ids, NativeId, accesskit::Node), TestError> {
    let mut tree = tree();
    let mut view = node(2, Role::ScrollView, "Tabs");
    view.actions = [ActionKind::Scroll(axis)].into();
    let mut item = node(3, Role::Button, "Tab");
    item.bounds = Rect::new(10.dp(), 20.dp(), 100.dp(), 12.dp());
    view.children.push(item.id);
    root_mut(&mut tree)?.children.push(view.id);
    assert!(tree.nodes.insert(view.id, view).is_none());
    assert!(tree.nodes.insert(item.id, item).is_none());
    let mut ids = ids::Ids::default();
    let native = mapping::tree(&tree, &mut ids, "");
    let target = ids.node(NodeId(2));
    let (_, mapped) = (native.nodes.into_iter())
        .find(|(id, _)| *id == target)
        .ok_or(TestError::MissingNativeNode(target))?;
    Ok((tree, ids, target, mapped))
}

#[test]
fn a_node_that_scrolls_sideways_has_horizontal_scroll_actions() -> Result<(), TestError> {
    let (tree, ids, target, mapped) = scroll_view(Axis::Horizontal)?;
    assert!(mapped.supports_action(Action::ScrollLeft));
    assert!(mapped.supports_action(Action::ScrollRight));
    assert!(!mapped.supports_action(Action::ScrollDown));
    let page = Some(ActionData::ScrollUnit(accesskit::ScrollUnit::Page));
    let action = actions::request(request(Action::ScrollLeft, target, page), &tree, &ids);
    assert_eq!(
        action.ok_or(TestError::RefusedAction)?.action,
        AccessibilityAction::Scroll(Axis::Horizontal, -100.dp())
    );
    let refused = actions::request(request(Action::ScrollDown, target, None), &tree, &ids);
    assert!(refused.is_none());
    Ok(())
}

#[test]
fn a_node_that_scrolls_up_and_down_has_vertical_scroll_actions() -> Result<(), TestError> {
    let (tree, ids, target, mapped) = scroll_view(Axis::Vertical)?;
    assert!(mapped.supports_action(Action::ScrollDown));
    assert!(!mapped.supports_action(Action::ScrollRight));
    let action = actions::request(request(Action::ScrollDown, target, None), &tree, &ids);
    assert_eq!(
        action.ok_or(TestError::RefusedAction)?.action,
        AccessibilityAction::Scroll(Axis::Vertical, 12.dp())
    );
    Ok(())
}

#[test]
fn removed_nodes_leave_parent_delta_and_stale_actions_are_rejected() -> Result<(), TestError> {
    let mut tree = tree();
    let mut ids = ids::Ids::default();
    let before = mapping::tree(&tree, &mut ids, "");
    let removed = ids.node(NodeId(1));
    assert!(tree.nodes.remove(&NodeId(1)).is_some());
    assert_eq!(root_mut(&mut tree)?.children.remove(0), NodeId(1));
    tree.focus = None;
    let after = mapping::tree(&tree, &mut ids, "");
    let native_root = after.tree.as_ref().ok_or(TestError::MissingTreeInfo)?.root;
    let delta = mapping::difference(&after, Some(&before));
    assert_eq!(delta.nodes.len(), 1);
    assert_eq!(delta.nodes[0].0, native_root);
    assert!(!delta.nodes[0].1.children().contains(&removed));
    assert!(actions::request(request(Action::Click, removed, None), &tree, &ids).is_none());
    assert_eq!(delta.focus, native_root);
    Ok(())
}

#[test]
fn unicode_selection_keeps_scalar_units_and_directed_endpoints() -> Result<(), TestError> {
    let tree = tree();
    let mut ids = ids::Ids::default();
    let native = mapping::tree(&tree, &mut ids, "");
    let field = native_node(&native, NativeRole::TextInput)?;
    let run = native_node(&native, NativeRole::TextRun)?;
    assert_eq!(run.1.character_lengths(), &[1, 4, 2]);
    let selection = field
        .1
        .text_selection()
        .ok_or(TestError::MissingSelection)?;
    assert_eq!(
        (
            selection.anchor.character_index,
            selection.focus.character_index
        ),
        (3, 1)
    );
    let action = actions::request(
        request(
            Action::SetTextSelection,
            field.0,
            Some(ActionData::SetTextSelection(*selection)),
        ),
        &tree,
        &ids,
    )
    .ok_or(TestError::RefusedAction)?;
    assert_eq!(
        action.action,
        AccessibilityAction::SetTextSelection(TextSelection {
            anchor: 3,
            focus: 1
        })
    );
    Ok(())
}

#[test]
fn shaped_selectable_units_convert_combining_and_zwj_text_without_splitting_editor_carets()
-> Result<(), TestError> {
    let mut tree = tree();
    let text = text_mut(&mut tree, NodeId(1 << 63))?;
    text.value = "ae\u{301}👩\u{200d}🚀ב".into();
    text.selection = Some(TextSelection {
        anchor: 7,
        focus: 3,
    });
    text.runs = vec![AccessibleTextRun {
        start: 0,
        text: text.value.clone(),
        selectable_lengths: vec![1, 2, 3, 1],
        direction: TextDirection::LeftToRight,
        character_bounds: (0..7_u16)
            .map(|i| Rect::new(10.dp() * f64::from(i), 0.dp(), 10.dp(), 20.dp()))
            .collect(),
        ..Default::default()
    }];
    let mut ids = ids::Ids::default();
    let native = mapping::tree(&tree, &mut ids, "");
    let field = native_node(&native, NativeRole::TextInput)?;
    let run = native_node(&native, NativeRole::TextRun)?;
    assert_eq!(run.1.character_lengths(), &[1, 3, 11, 2]);
    assert_eq!(
        run.1.character_widths(),
        Some([10.0, 20.0, 30.0, 10.0].as_slice())
    );
    let selection = field
        .1
        .text_selection()
        .ok_or(TestError::MissingSelection)?;
    assert_eq!(
        (
            selection.anchor.character_index,
            selection.focus.character_index
        ),
        (4, 2)
    );
    let action = actions::request(
        request(
            Action::SetTextSelection,
            field.0,
            Some(ActionData::SetTextSelection(*selection)),
        ),
        &tree,
        &ids,
    )
    .ok_or(TestError::RefusedAction)?;
    assert_eq!(
        action.action,
        AccessibilityAction::SetTextSelection(TextSelection {
            anchor: 7,
            focus: 3
        })
    );
    Ok(())
}

#[test]
fn right_to_left_geometry_uses_resolved_run_direction() -> Result<(), TestError> {
    let mut tree = tree();
    let text = text_mut(&mut tree, NodeId(1 << 63))?;
    text.value = "אבג".into();
    text.selection = None;
    text.runs = vec![AccessibleTextRun {
        start: 0,
        text: text.value.clone(),
        selectable_lengths: vec![1, 1, 1],
        direction: TextDirection::RightToLeft,
        character_bounds: [30.dp(), 20.dp(), 10.dp()]
            .map(|x| Rect::new(x, 0.dp(), 10.dp(), 20.dp()))
            .into(),
        ..Default::default()
    }];
    let native = mapping::tree(&tree, &mut ids::Ids::default(), "");
    let run = native_node(&native, NativeRole::TextRun)?;
    assert_eq!(
        run.1.text_direction(),
        Some(accesskit::TextDirection::RightToLeft)
    );
    assert_eq!(
        run.1.character_positions(),
        Some([0.0, 10.0, 20.0].as_slice())
    );
    Ok(())
}

#[test]
fn unadvertised_disabled_and_non_finite_actions_do_nothing() -> Result<(), TestError> {
    let mut tree = tree();
    let mut ids = ids::Ids::default();
    let native = mapping::tree(&tree, &mut ids, "");
    let button = ids.node(NodeId(1));
    assert!(native.nodes.iter().any(|(id, _)| *id == button));
    assert!(actions::request(request(Action::Click, button, None), &tree, &ids).is_some());
    assert!(
        actions::request(
            request(
                Action::SetValue,
                button,
                Some(ActionData::NumericValue(1.0))
            ),
            &tree,
            &ids
        )
        .is_none()
    );
    assert!(
        node_mut(&mut tree, NodeId(1))?
            .actions
            .insert(ActionKind::SetValue)
    );
    assert!(
        actions::request(
            request(
                Action::SetValue,
                button,
                Some(ActionData::NumericValue(f64::NAN))
            ),
            &tree,
            &ids
        )
        .is_none()
    );
    node_mut(&mut tree, NodeId(1))?.state.semantics.disabled = true;
    assert!(actions::request(request(Action::Click, button, None), &tree, &ids).is_none());
    Ok(())
}

#[test]
fn unchanged_live_regions_produce_no_nodes_in_incremental_update() -> Result<(), TestError> {
    let mut tree = tree();
    node_mut(&mut tree, NodeId(1))?.live = LiveRegion::Assertive;
    let mut ids = ids::Ids::default();
    let before = mapping::tree(&tree, &mut ids, "");
    let after = mapping::tree(&tree, &mut ids, "");
    assert!(mapping::difference(&after, Some(&before)).nodes.is_empty());
    node_mut(&mut tree, NodeId(1))?.name = Some("Changed".into());
    let after = mapping::tree(&tree, &mut ids, "");
    let delta = mapping::difference(&after, Some(&before));
    assert_eq!(delta.nodes.len(), 1);
    assert_eq!(delta.nodes[0].1.live(), Some(accesskit::Live::Assertive));
    Ok(())
}

#[test]
fn transport_maps_relations_checked_ranges_and_virtualized_positions() -> Result<(), TestError> {
    let mut tree = tree();
    let source = node_mut(&mut tree, NodeId(1))?;
    source.role = Role::Checkbox;
    source.state.semantics.checked = Some(Checked::Mixed);
    source.state.selected = Some(false);
    source.state.semantics.form.required = true;
    source.range = Some(RangeInfo {
        min: 0.0,
        max: 100.0,
        step: Some(5.0),
    });
    source.relations.labelled_by = vec![NodeId(1 << 63)];
    source.item = Some(CollectionItem {
        row: 99,
        column: 0,
        row_span: 1,
        column_span: 1,
        heading: false,
    });
    let mut ids = ids::Ids::default();
    let native = mapping::tree(&tree, &mut ids, "");
    let nodes: BTreeMap<_, _> = native.nodes.into_iter().collect();
    let node = &nodes[&ids.node(NodeId(1))];
    assert_eq!(node.role(), NativeRole::CheckBox);
    assert_eq!(node.toggled(), Some(accesskit::Toggled::Mixed));
    assert_eq!(node.is_selected(), Some(false));
    assert!(node.is_required());
    assert_eq!(node.max_numeric_value(), Some(100.0));
    assert_eq!(node.numeric_value_step(), Some(5.0));
    assert_eq!(node.position_in_set(), Some(99));
    assert_eq!(node.labelled_by(), &[ids.node(NodeId(1 << 63))]);
    Ok(())
}

#[test]
fn heading_levels_and_grid_item_positions_use_portable_logical_indices() -> Result<(), TestError> {
    let mut tree = tree();
    root_mut(&mut tree)?.collection = Some(CollectionInfo {
        rows: Some(34),
        columns: Some(3),
        item_count: Some(100),
        multiple_selection: false,
    });
    root_mut(&mut tree)?.role = Role::List;
    let source = node_mut(&mut tree, NodeId(1))?;
    source.role = Role::Heading;
    source.heading_level = Some(3);
    source.item = Some(CollectionItem {
        row: 32,
        column: 2,
        ..CollectionItem::default()
    });
    let mut ids = ids::Ids::default();
    let native = mapping::tree(&tree, &mut ids, "");
    let nodes: BTreeMap<_, _> = native.nodes.into_iter().collect();
    assert_eq!(nodes[&ids.node(tree.root)].size_of_set(), Some(100));
    assert_eq!(nodes[&ids.node(NodeId(1))].position_in_set(), Some(98));
    assert_eq!(nodes[&ids.node(NodeId(1))].level(), Some(2));
    Ok(())
}

#[test]
fn worker_requests_wake_idle_event_loop_and_activation_always_returns_full_tree()
-> Result<(), TestError> {
    use accesskit::{ActionHandler, ActivationHandler};
    use sctk::reexports::calloop::{EventLoop, channel};
    let mut loop_: EventLoop<Vec<Event>> = EventLoop::try_new()?;
    let (sender, receiver) = channel::channel();
    let requests = loop_
        .handle()
        .insert_source(receiver, |event, (), events| match event {
            channel::Event::Msg(event) => events.push(event),
            channel::Event::Closed => assert_eq!(events.len(), 3, "the worker ended early"),
        })?;
    let full = mapping::tree(&tree(), &mut ids::Ids::default(), "");
    let snapshot = Arc::new(Mutex::new(Some(full.clone())));
    let worker = std::thread::spawn(move || {
        let mut activation = handlers::Activation::new(7, sender.clone(), snapshot);
        assert_eq!(activation.request_initial_tree(), Some(full.clone()));
        assert_eq!(activation.request_initial_tree(), Some(full));
        handlers::Actions::new(7, sender).do_action(request(Action::Click, NativeId(2), None));
    });
    let mut events = Vec::new();
    while events.len() < 3 {
        loop_.dispatch(None, &mut events)?;
    }
    if let Err(panic) = worker.join() {
        std::panic::resume_unwind(panic);
    }
    assert!(matches!(events[0], Event::Activate(7)));
    assert!(matches!(events[1], Event::Activate(7)));
    assert!(matches!(events[2], Event::Action(7, _)));
    loop_.handle().remove(requests);
    Ok(())
}
