// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use lavashell::core::scene::tree::{Content, Node, Packet, SceneNodeId};
use std::sync::Arc;

fn top_leaf() -> Change {
    Change::Insert {
        id: SceneNodeId(1),
        parent: None,
        index: 0,
        node: Node::Leaf(Content::Packet(Packet::new(Arc::from([])))),
    }
}

#[test]
fn a_frame_changes_the_scene_when_it_restarts_or_receives_a_change() -> Result<(), ApplyError> {
    let mut scene = OffscreenScene::default();
    scene.restart()?;
    scene.end_frame()?;
    assert!(scene.last_frame_changed);
    scene.end_frame()?;
    assert!(!scene.last_frame_changed);
    scene.change(top_leaf())?;
    scene.end_frame()?;
    assert!(scene.last_frame_changed);
    scene.end_frame()?;
    assert!(!scene.last_frame_changed);
    Ok(())
}
