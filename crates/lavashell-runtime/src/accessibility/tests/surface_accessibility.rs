// SPDX-License-Identifier: GPL-3.0-only
use super::*;

const SURFACE: SurfaceId = SurfaceId::numbered(1);

fn published_tree(
    request: Option<AccessibilityRequest>,
) -> Result<Box<TreeUpdate>, AccessibilityTestError> {
    match request {
        Some(AccessibilityRequest::Publish { tree, .. }) => Ok(tree),
        _ => Err(AccessibilityTestError::NotPublished),
    }
}

#[test]
fn a_surface_publishes_only_while_an_assistive_technology_reads_a_tree_with_nodes()
-> Result<(), AccessibilityTestError> {
    let mut state = SurfaceAccessibility::new(Some("Window".into()));
    let tree = tree();
    assert_eq!(state.publish(SURFACE, &tree, Republish::Always)?, None);
    state.activate();
    assert_ne!(state.publish(SURFACE, &tree, Republish::Always)?, None);
    state.deactivate();
    assert_eq!(state.publish(SURFACE, &tree, Republish::Always)?, None);
    state.activate();
    let empty = AccessibilityTree::default();
    assert_eq!(state.publish(SURFACE, &empty, Republish::Always)?, None);
    Ok(())
}

#[test]
fn a_surface_publishes_each_revision_once_and_again_when_asked()
-> Result<(), AccessibilityTestError> {
    let mut state = SurfaceAccessibility::new(Some("Window".into()));
    state.activate();
    let mut tree = tree();
    assert_ne!(state.publish(SURFACE, &tree, Republish::WhenChanged)?, None);
    assert_eq!(state.publish(SURFACE, &tree, Republish::WhenChanged)?, None);
    node_mut(&mut tree, NodeId(1))?.name = Some("Changed".into());
    tree.revision += 1;
    let changed = state.publish(SURFACE, &tree, Republish::WhenChanged)?;
    let changed = published_tree(changed)?;
    let again = state.publish(SURFACE, &tree, Republish::Always)?;
    assert_eq!(published_tree(again)?, changed);
    Ok(())
}

#[test]
fn a_root_without_a_name_has_the_name_of_the_surface() -> Result<(), AccessibilityTestError> {
    let mut tree = tree();
    root_mut(&mut tree)?.name = None;
    let mut state = SurfaceAccessibility::new(Some("Window".into()));
    state.activate();
    let published = published_tree(state.publish(SURFACE, &tree, Republish::Always)?)?;
    let root = &native_node(&published, NativeRole::Window)?.1;
    assert_eq!(root.label(), Some("Window"));
    Ok(())
}

#[test]
fn a_surface_maps_actions_only_while_an_assistive_technology_reads_it()
-> Result<(), AccessibilityTestError> {
    let tree = tree();
    let mut state = SurfaceAccessibility::new(None);
    state.activate();
    let button = state.ids.node(NodeId(1))?;
    let click = || request(Action::Click, button, None);
    assert_ne!(state.publish(SURFACE, &tree, Republish::Always)?, None);
    assert!(state.action(click(), &tree).is_some());
    state.deactivate();
    assert_eq!(state.action(click(), &tree), None);
    Ok(())
}
