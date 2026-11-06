// SPDX-License-Identifier: GPL-3.0-only
use super::tree::{ApplyError, Change, Content, Node, Packet, SceneNodeId, SceneSnapshot};
use super::{ChangeSink, DrawCommand, SceneBuilder};
use crate::geometry::{Color, Rect};
use crate::test_error::TestResult;
use crate::units::DpExt;
use std::sync::Arc;
use std::time::Duration;

fn leaf(id: u64) -> Change {
    let fill = DrawCommand::Fill {
        shape: Rect::new(0.dp(), 0.dp(), 10.dp(), 10.dp()).into(),
        brush: Color::WHITE.into(),
    };
    Change::Insert {
        id: SceneNodeId(id),
        parent: None,
        index: 0,
        node: Node::Leaf(Content::Packet(Packet::new(Arc::from([fill])))),
    }
}

fn leaves(snapshot: &SceneSnapshot) -> Vec<SceneNodeId> {
    (snapshot.at(Duration::ZERO).paint_order().leaves())
        .map(|leaf| leaf.key.node)
        .collect()
}

#[test]
fn a_snapshot_shows_the_changes_of_a_frame_once_the_frame_ends() -> TestResult {
    let mut builder = SceneBuilder::default();
    builder.change(leaf(1))?;
    assert!(builder.is_empty() && leaves(&builder.snapshot()).is_empty());
    builder.end_frame()?;
    assert_eq!(leaves(&builder.snapshot()), [SceneNodeId(1)]);
    Ok(())
}

#[test]
fn a_change_that_does_not_fit_leaves_the_snapshot_at_the_last_ended_frame() -> TestResult {
    let mut builder = SceneBuilder::default();
    builder.change(leaf(1))?;
    builder.end_frame()?;
    builder.change(leaf(2))?;
    assert_eq!(
        builder.change(leaf(2)),
        Err(ApplyError::DuplicateNode(SceneNodeId(2)))
    );
    assert_eq!(leaves(&builder.snapshot()), [SceneNodeId(1)]);
    Ok(())
}

#[test]
fn a_restart_after_a_change_that_does_not_fit_builds_the_next_frame_from_empty() -> TestResult {
    let mut builder = SceneBuilder::default();
    builder.change(leaf(1))?;
    builder.end_frame()?;
    assert_eq!(
        builder.change(leaf(1)),
        Err(ApplyError::DuplicateNode(SceneNodeId(1)))
    );
    builder.restart()?;
    assert_eq!(leaves(&builder.snapshot()), [SceneNodeId(1)]);
    builder.change(leaf(2))?;
    builder.end_frame()?;
    assert_eq!(leaves(&builder.snapshot()), [SceneNodeId(2)]);
    Ok(())
}

#[test]
fn snapshots_share_a_lineage_until_the_builder_restarts() -> TestResult {
    let mut builder = SceneBuilder::default();
    builder.change(leaf(1))?;
    builder.end_frame()?;
    let first = builder.snapshot().lineage();
    builder.change(leaf(2))?;
    builder.end_frame()?;
    assert_eq!(builder.snapshot().lineage(), first);
    builder.restart()?;
    assert_eq!(builder.snapshot().lineage(), first);
    builder.end_frame()?;
    assert_ne!(builder.snapshot().lineage(), first);
    Ok(())
}

#[test]
fn a_restart_empties_the_snapshot_once_the_next_frame_ends() -> TestResult {
    let mut builder = SceneBuilder::default();
    builder.change(leaf(1))?;
    builder.end_frame()?;
    builder.restart()?;
    assert!(!builder.is_empty());
    assert_eq!(leaves(&builder.snapshot()), [SceneNodeId(1)]);
    builder.end_frame()?;
    assert!(builder.is_empty() && leaves(&builder.snapshot()).is_empty());
    Ok(())
}
