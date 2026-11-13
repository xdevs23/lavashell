// SPDX-License-Identifier: GPL-3.0-only
use super::test_changes::{insert, show_all};
use super::*;
use crate::geometry::{Color, Point};
use crate::scene::Affine;
use crate::test_error::{Required, TestResult};
use crate::units::DpExt;
use std::time::Duration;

fn fill() -> DrawCommand {
    DrawCommand::Fill {
        shape: Rect::new(0.dp(), 0.dp(), 10.dp(), 10.dp()).into(),
        brush: Color::WHITE.into(),
    }
}

fn packet() -> Packet {
    Packet::new(Arc::from([fill()]))
}

fn leaf() -> Node {
    Node::Leaf(Content::Packet(packet()))
}

#[derive(Debug, PartialEq)]
enum Seen {
    Group(u64),
    Graft(u64),
    Leaf(u64),
    Exit,
}

use Seen::{Exit, Graft as G, Group as Gr, Leaf as L};

struct Recorder(Vec<Seen>);

impl PaintVisitor<'_> for Recorder {
    type Saved = ();

    fn group(&mut self, key: NodeKey, _: GroupValues, _: Affine) {
        self.0.push(Gr(key.node.0));
    }

    fn graft(&mut self, key: NodeKey, _: NamespaceId) {
        self.0.push(G(key.node.0));
    }

    fn leaf(&mut self, leaf: PlacedLeaf<'_>) {
        self.0.push(L(leaf.key.node.0));
    }

    fn exit(&mut self, (): ()) {
        self.0.push(Exit);
    }
}

fn seen(tree: &SceneTree) -> Vec<Seen> {
    seen_in(tree.at(Duration::ZERO))
}

fn seen_in(scene: Presented<'_>) -> Vec<Seen> {
    let mut recorder = Recorder(Vec::new());
    scene.paint_order().walk(&mut recorder);
    recorder.0
}

/// Group 1 contains leaf 2 and group 3, which contains leaf 4. Leaf 5 follows group 1.
fn nested() -> Result<SceneTree, ApplyError> {
    let mut tree = SceneTree::default();
    let changes = [
        insert(1, None, 0, Node::Group(Group::default())),
        insert(2, Some(1), 0, leaf()),
        insert(3, Some(1), 1, Node::Group(Group::default())),
        insert(4, Some(3), 0, leaf()),
        insert(5, None, 1, leaf()),
    ];
    show_all(&mut tree, changes)?;
    Ok(tree)
}

#[test]
fn inserts_moves_and_removals_preserve_the_paint_order_of_the_tree() -> TestResult {
    let mut tree = nested()?;
    assert_eq!(seen(&tree), [Gr(1), L(2), Gr(3), L(4), Exit, Exit, L(5)]);
    let moved = Change::Move {
        id: SceneNodeId(5),
        parent: Some(SceneNodeId(3)),
        index: 0,
    };
    tree.show_change(moved)?;
    assert_eq!(seen(&tree), [Gr(1), L(2), Gr(3), L(5), L(4), Exit, Exit]);
    let removed = Change::Remove { id: SceneNodeId(3) };
    tree.show_change(removed)?;
    assert_eq!(seen(&tree), [Gr(1), L(2), Exit]);
    // Removing group 3 also removed leaves 4 and 5. An insert below group 3 fails, while ID 4 is free for a new leaf.
    let under_removed = insert(6, Some(3), 0, leaf());
    let error = tree.show_change(under_removed);
    assert_eq!(error, Err(ApplyError::UnknownNode(SceneNodeId(3))));
    tree.show_change(insert(4, None, 1, leaf()))?;
    assert_eq!(seen(&tree), [Gr(1), L(2), Exit, L(4)]);
    Ok(())
}

#[test]
fn changes_that_would_break_the_structure_fail_and_leave_the_tree_as_it_was() -> TestResult {
    let mut tree = nested()?;
    let invalid = [
        (
            Change::Move {
                id: SceneNodeId(1),
                parent: Some(SceneNodeId(3)),
                index: 0,
            },
            ApplyError::Cycle(SceneNodeId(1)),
        ),
        (
            insert(6, Some(2), 0, leaf()),
            ApplyError::NotAGroup(SceneNodeId(2)),
        ),
        (
            insert(2, None, 0, leaf()),
            ApplyError::DuplicateNode(SceneNodeId(2)),
        ),
        (
            insert(6, None, 3, leaf()),
            ApplyError::IndexPastEnd {
                index: 3,
                children: 2,
            },
        ),
        (
            Change::Move {
                id: SceneNodeId(5),
                parent: None,
                index: 2,
            },
            ApplyError::IndexPastEnd {
                index: 2,
                children: 1,
            },
        ),
        (
            Change::SetSlot {
                id: SceneNodeId(2),
                slot: SlotChange::Opacity(Slot::Value(0.5)),
            },
            ApplyError::NotAGroup(SceneNodeId(2)),
        ),
        (
            Change::SetContent {
                id: SceneNodeId(1),
                content: Content::Graft(SceneTree::ROOT),
            },
            ApplyError::NotALeaf(SceneNodeId(1)),
        ),
    ];
    for (change, error) in invalid {
        assert_eq!(tree.show_change(change), Err(error));
    }
    assert_eq!(seen(&tree), [Gr(1), L(2), Gr(3), L(4), Exit, Exit, L(5)]);
    Ok(())
}

#[test]
fn a_parent_in_another_namespace_fails() -> TestResult {
    let mut tree = SceneTree::default();
    let other = tree.create_namespace();
    let group = insert(7, None, 0, Node::Group(Group::default()));
    tree.apply_change(other, group)?;
    let error = tree.show_change(insert(8, Some(7), 0, leaf()));
    assert_eq!(error, Err(ApplyError::UnknownNode(SceneNodeId(7))));
    Ok(())
}

#[test]
fn a_change_to_an_unknown_namespace_fails_and_leaves_the_tree_as_it_was() -> TestResult {
    let mut tree = nested()?;
    let mut elsewhere = SceneTree::default();
    let _first = elsewhere.create_namespace();
    let unknown = elsewhere.create_namespace();
    let changes = [
        insert(9, None, 0, leaf()),
        Change::Remove { id: SceneNodeId(1) },
        Change::Move {
            id: SceneNodeId(1),
            parent: None,
            index: 0,
        },
        Change::SetContent {
            id: SceneNodeId(2),
            content: Content::Packet(packet()),
        },
        Change::SetSlot {
            id: SceneNodeId(1),
            slot: SlotChange::Opacity(Slot::Value(0.5)),
        },
    ];
    for change in changes {
        let error = tree.apply_change(unknown, change);
        assert_eq!(error, Err(ApplyError::UnknownNamespace(unknown)));
    }
    assert_eq!(seen(&tree), [Gr(1), L(2), Gr(3), L(4), Exit, Exit, L(5)]);
    Ok(())
}

#[test]
fn a_graft_paints_its_namespace_in_place_once_and_never_inside_itself() -> TestResult {
    let mut tree = SceneTree::default();
    let grafted = tree.create_namespace();
    tree.apply_change(grafted, insert(1, None, 0, leaf()))?;
    let graft = |id| insert(id, None, 0, Node::Leaf(Content::Graft(grafted)));
    tree.show_change(graft(9))?;
    assert_eq!(seen(&tree), [G(9), L(1), Exit]);
    let error = tree.show_change(graft(10));
    assert_eq!(error, Err(ApplyError::AlreadyGrafted(grafted)));
    let back = insert(2, None, 0, Node::Leaf(Content::Graft(SceneTree::ROOT)));
    let error = tree.apply_change(grafted, back);
    assert_eq!(error, Err(ApplyError::GraftCycle(SceneTree::ROOT)));
    // `inner` is grafted below `grafted`, so a graft of `grafted` inside `inner` fails. It fails first because node 9
    // already grafts `grafted`, and after node 9 is removed because the graft would paint `grafted` inside itself.
    let inner = tree.create_namespace();
    let graft_inner = insert(3, None, 0, Node::Leaf(Content::Graft(inner)));
    tree.apply_change(grafted, graft_inner)?;
    let graft_outer = || insert(1, None, 0, Node::Leaf(Content::Graft(grafted)));
    let error = tree.apply_change(inner, graft_outer());
    assert_eq!(error, Err(ApplyError::AlreadyGrafted(grafted)));
    let ungraft = Change::Remove { id: SceneNodeId(9) };
    tree.show_change(ungraft)?;
    let error = tree.apply_change(inner, graft_outer());
    assert_eq!(error, Err(ApplyError::GraftCycle(grafted)));
    Ok(())
}

#[test]
fn the_world_transform_of_a_leaf_applies_the_innermost_group_first() -> TestResult {
    let mut tree = SceneTree::default();
    let group = |transform| {
        Node::Group(Group {
            transform: Slot::Value(transform),
            ..Group::default()
        })
    };
    let changes = [
        insert(
            1,
            None,
            0,
            group(Affine::translate(Point::new(10.dp(), 0.dp()))),
        ),
        insert(2, Some(1), 0, group(Affine::scale(2.0, 2.0))),
        insert(3, Some(2), 0, leaf()),
    ];
    show_all(&mut tree, changes)?;
    let world = tree
        .at(Duration::ZERO)
        .paint_order()
        .leaves()
        .next()
        .required("leaf")?
        .world;
    assert_eq!(
        world.map_point(Point::new(1.dp(), 1.dp())),
        Point::new(12.dp(), 2.dp())
    );
    Ok(())
}

#[test]
fn a_tree_with_one_leaf_shows_it_and_applies_changes_of_that_leaf() -> TestResult {
    let mut tree = SceneTree::with_leaf([fill()])?;
    assert_eq!(seen(&tree), [L(SceneTree::LEAF.node.0)]);
    let replaced = Change::SetContent {
        id: SceneTree::LEAF.node,
        content: Content::Packet(packet()),
    };
    tree.show_change(replaced)?;
    assert_eq!(seen(&tree), [L(SceneTree::LEAF.node.0)]);
    Ok(())
}

#[test]
fn every_packet_gets_a_generation_larger_than_those_made_before_it() {
    let first = packet();
    let second = packet();
    assert!(second.generation() > first.generation());
}

#[test]
fn a_snapshot_keeps_showing_its_moment_while_the_tree_changes() -> TestResult {
    let mut tree = nested()?;
    let snapshot = tree.snapshot();
    let changes = [
        Change::Move {
            id: SceneNodeId(5),
            parent: Some(SceneNodeId(3)),
            index: 0,
        },
        Change::Remove { id: SceneNodeId(2) },
        insert(6, Some(1), 0, leaf()),
        Change::SetSlot {
            id: SceneNodeId(1),
            slot: SlotChange::Opacity(Slot::Value(0.5)),
        },
    ];
    show_all(&mut tree, changes)?;
    assert_eq!(seen(&tree), [Gr(1), L(6), Gr(3), L(5), L(4), Exit, Exit]);
    let shown = seen_in(snapshot.at(Duration::ZERO));
    assert_eq!(shown, [Gr(1), L(2), Gr(3), L(4), Exit, Exit, L(5)]);
    let half_opaque = |scene: Presented<'_>| scene.commands().contains(&DrawCommand::Opacity(0.5));
    assert!(half_opaque(tree.at(Duration::ZERO)));
    assert!(!half_opaque(snapshot.at(Duration::ZERO)));
    Ok(())
}

#[test]
fn snapshots_share_the_nodes_that_the_tree_did_not_change_between_them() -> TestResult {
    let mut tree = nested()?;
    let before = tree.snapshot();
    let replaced = Change::SetContent {
        id: SceneNodeId(4),
        content: Content::Packet(packet()),
    };
    tree.show_change(replaced)?;
    let after = tree.snapshot();
    let packets = |snapshot: &SceneSnapshot| -> Vec<*const Packet> {
        (snapshot.at(Duration::ZERO).paint_order().leaves())
            .map(|leaf| std::ptr::from_ref(leaf.packet))
            .collect()
    };
    let shared: Vec<bool> = (packets(&before).into_iter())
        .zip(packets(&after))
        .map(|(old, new)| old == new)
        .collect();
    assert_eq!(shared, [true, false, true]);
    Ok(())
}
