// SPDX-License-Identifier: GPL-3.0-only
use super::apply_frame;
use crate::test_error::TestError;
use lavashell_core::scene::tree::{
    ApplyError, Change, Content, Node, Packet, SceneNodeId, SceneTree, SceneUpdate,
};
use lavashell_core::view::FrameError;
use std::sync::Arc;

/// An update that inserts a leaf without commands as node 1 at the top of a namespace.
fn leaf() -> SceneUpdate {
    let node = Node::Leaf(Content::Packet(Packet::new(Arc::from([]))));
    SceneUpdate::from_iter([Change::Insert {
        id: SceneNodeId(1),
        parent: None,
        index: 0,
        node,
    }])
}

const FAILED: ApplyError = ApplyError::UnknownNode(SceneNodeId(9));

fn failing(_: &mut SceneTree) -> Result<(), FrameError> {
    Err(FrameError::Apply(FAILED))
}

#[test]
fn a_failed_apply_fails_while_only_produced_namespaces_show() -> Result<(), TestError> {
    let mut tree = SceneTree::default();
    let produced = tree.create_namespace();
    tree.apply(produced.id(), leaf())?;
    assert_eq!(
        apply_frame(&mut tree, failing),
        Err(FrameError::Apply(FAILED))
    );
    Ok(())
}

#[test]
fn a_failed_apply_after_the_root_showed_a_scene_restarts() -> Result<(), TestError> {
    let mut tree = SceneTree::default();
    tree.show(leaf())?;
    assert_eq!(apply_frame(&mut tree, failing), Ok(Some(FAILED)));
    Ok(())
}
