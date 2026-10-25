// SPDX-License-Identifier: GPL-3.0-only
use super::super::{GroupValues, placed_at};
use super::*;
use crate::geometry::{Color, Point, Rect};
use crate::scene::DrawCommand;
use crate::scene::tree::{NamespaceId, PaintVisitor, PlacedLeaf};
use crate::test_error::TestResult;
use crate::ui::NodeId;
use crate::units::{Dp, DpExt};
use std::time::Duration;

fn fill(x: f64) -> DrawCommand {
    DrawCommand::Fill {
        shape: Rect::new(Dp::new(x), Dp::ZERO, 4.dp(), 4.dp()).into(),
        brush: Color::WHITE.into(),
    }
}

fn leaf(x: f64) -> Painted {
    Painted::Leaf {
        commands: Arc::from([fill(x)]),
    }
}

fn component(id: u64, x: f64, items: &Arc<[Painted]>) -> Painted {
    Painted::Group {
        id: GroupId::Component(NodeId(id)),
        group: placed_at(Point::new(Dp::new(x), Dp::ZERO)),
        items: items.clone(),
    }
}

fn faded(opacity: f64, items: Vec<Painted>) -> Painted {
    Painted::Scope {
        values: GroupValues {
            opacity,
            ..GroupValues::default()
        },
        items,
    }
}

#[derive(Debug, PartialEq)]
enum Kind {
    Insert,
    Remove,
    Move,
    SetContent,
    SetSlot,
}

/// The nodes of a scene in paint order: each group by its values, followed by its items and an `Exit`, and each
/// leaf by its commands.
#[derive(Debug, PartialEq)]
enum Shape {
    Group(GroupValues),
    Graft(NamespaceId),
    Leaf(Vec<DrawCommand>),
    Exit,
}

fn expected(items: &[Painted], shapes: &mut Vec<Shape>) {
    for item in items {
        let (values, inner): (_, &[Painted]) = match item {
            Painted::Leaf { commands, .. } => {
                shapes.push(Shape::Leaf(commands.to_vec()));
                continue;
            }
            Painted::Scope { values, items } => (*values, items),
            Painted::Group { group, items, .. } => (group.at(Duration::ZERO), items),
        };
        shapes.push(Shape::Group(values));
        expected(inner, shapes);
        shapes.push(Shape::Exit);
    }
}

#[derive(Default)]
struct Consumer {
    shown: Shown,
    tree: SceneTree,
}

impl Consumer {
    /// Checks that the tree shows `items` after the update, and returns the kinds of the changes.
    fn show(&mut self, items: &[Painted]) -> Result<Vec<Kind>, ApplyError> {
        self.shown.retarget(Recipient::Tree(self.tree.id()));
        let update = self.shown.update(items, &BTreeMap::new())?;
        let kinds = (update.changes.iter())
            .map(|change| match change {
                Change::Insert { .. } => Kind::Insert,
                Change::Remove { .. } => Kind::Remove,
                Change::Move { .. } => Kind::Move,
                Change::SetContent { .. } => Kind::SetContent,
                Change::SetSlot { .. } => Kind::SetSlot,
            })
            .collect();
        self.tree.show(update)?;
        let mut shown = Shapes(Vec::new());
        self.tree.at(Duration::ZERO).paint_order().walk(&mut shown);
        let mut shapes = Vec::new();
        expected(items, &mut shapes);
        assert_eq!(shown.0, shapes);
        Ok(kinds)
    }
}

struct Shapes(Vec<Shape>);

impl PaintVisitor<'_> for Shapes {
    type Saved = ();

    fn group(&mut self, _: NodeKey, values: GroupValues, _: Affine) {
        self.0.push(Shape::Group(values));
    }

    fn graft(&mut self, _: NodeKey, namespace: NamespaceId) {
        self.0.push(Shape::Graft(namespace));
    }

    fn leaf(&mut self, leaf: PlacedLeaf<'_>) {
        self.0.push(Shape::Leaf(leaf.packet.commands().to_vec()));
    }

    fn exit(&mut self, (): ()) {
        self.0.push(Shape::Exit);
    }
}

#[test]
fn the_first_update_inserts_every_item_and_the_same_items_change_nothing() -> TestResult {
    let card: Arc<[Painted]> = Arc::from([leaf(1.0), faded(0.5, vec![leaf(2.0)])]);
    let mut consumer = Consumer::default();
    let first = consumer.show(&[leaf(0.0), component(7, 10.0, &card)])?;
    assert!(first.iter().all(|kind| *kind == Kind::Insert));
    assert_eq!(first.len(), 5);
    assert_eq!(consumer.show(&[leaf(0.0), component(7, 10.0, &card)])?, []);
    Ok(())
}

#[test]
fn a_component_moved_with_the_same_items_changes_only_its_transform() -> TestResult {
    let card: Arc<[Painted]> = Arc::from([leaf(1.0)]);
    let mut consumer = Consumer::default();
    assert!(
        consumer
            .show(&[component(7, 10.0, &card)])?
            .iter()
            .all(|kind| *kind == Kind::Insert)
    );
    assert_eq!(
        consumer.show(&[component(7, 30.0, &card)])?,
        [Kind::SetSlot]
    );
    Ok(())
}

#[test]
fn a_repainted_leaf_reuses_the_packet_while_the_commands_stay_equal() -> TestResult {
    let mut consumer = Consumer::default();
    let card: Arc<[Painted]> = Arc::from([leaf(1.0), leaf(2.0)]);
    assert!(
        consumer
            .show(&[component(7, 0.0, &card)])?
            .iter()
            .all(|kind| *kind == Kind::Insert)
    );
    let repainted: Arc<[Painted]> = Arc::from([leaf(1.0), leaf(3.0)]);
    assert_eq!(
        consumer.show(&[component(7, 0.0, &repainted)])?,
        [Kind::SetContent]
    );
    Ok(())
}

#[test]
fn reordered_and_removed_items_move_and_remove_their_nodes() -> TestResult {
    let content = |x| Arc::from([leaf(x)]);
    let (a, b, c) = (content(1.0), content(2.0), content(3.0));
    let mut consumer = Consumer::default();
    assert!(
        consumer
            .show(&[
                component(1, 0.0, &a),
                component(2, 0.0, &b),
                component(3, 0.0, &c),
            ])?
            .iter()
            .all(|kind| *kind == Kind::Insert)
    );
    assert_eq!(
        consumer.show(&[component(3, 0.0, &c), component(1, 0.0, &a)])?,
        [Kind::Move, Kind::Remove]
    );
    Ok(())
}

/// The leaf is still the first leaf of the component. The update therefore moves the shown leaf into the new scope
/// instead of inserting a new leaf.
#[test]
fn a_leaf_moves_into_a_scope_opened_around_it() -> TestResult {
    let mut consumer = Consumer::default();
    let plain: Arc<[Painted]> = Arc::from([leaf(1.0)]);
    assert!(
        consumer
            .show(&[component(7, 0.0, &plain)])?
            .iter()
            .all(|kind| *kind == Kind::Insert)
    );
    let inside: Arc<[Painted]> = Arc::from([faded(0.5, vec![leaf(1.0)])]);
    assert_eq!(
        consumer.show(&[component(7, 0.0, &inside)])?,
        [Kind::Insert, Kind::Move]
    );
    Ok(())
}

#[test]
fn a_new_tree_is_shown_the_whole_scene_with_new_ids_and_generations() -> TestResult {
    let card: Arc<[Painted]> = Arc::from([leaf(1.0)]);
    let items = [leaf(0.0), component(7, 0.0, &card)];
    // The node id and the generation of each leaf shown by the tree.
    let leaves = |tree: &SceneTree| {
        (tree.at(Duration::ZERO).paint_order().leaves())
            .map(|leaf| (leaf.key.node, leaf.packet.generation()))
            .collect::<Vec<_>>()
    };
    let mut consumer = Consumer::default();
    assert!(
        consumer
            .show(&items)?
            .iter()
            .all(|kind| *kind == Kind::Insert)
    );
    let old = leaves(&consumer.tree);
    consumer.tree = SceneTree::default();
    assert!(
        consumer
            .show(&items)?
            .iter()
            .all(|kind| *kind == Kind::Insert)
    );
    let new = leaves(&consumer.tree);
    assert_eq!(new.len(), old.len());
    assert!(new.iter().all(|(id, generation)| {
        (old.iter()).all(|(old_id, old_generation)| id > old_id && generation > old_generation)
    }));
    Ok(())
}

#[test]
fn a_failed_change_forgets_the_scene_and_the_next_update_inserts_all_of_it() -> TestResult {
    let card: Arc<[Painted]> = Arc::from([leaf(1.0)]);
    let items = [component(7, 0.0, &card)];
    let mut consumer = Consumer::default();
    // The view inserts the group as node 1 and the leaf inside it as node 2.
    let planted = Change::Insert {
        id: SceneNodeId(2),
        parent: None,
        index: 0,
        node: Node::Group(Group::default()),
    };
    consumer.shown.tree.show_change(planted)?;
    let failed = consumer.shown.update(&items, &BTreeMap::new());
    assert_eq!(
        failed.err(),
        Some(ApplyError::DuplicateNode(SceneNodeId(2)))
    );
    assert!(consumer.shown.items.is_empty() && consumer.shown.ids.is_empty());
    assert!(
        consumer
            .show(&items)?
            .iter()
            .all(|kind| *kind == Kind::Insert)
    );
    Ok(())
}

#[test]
fn items_painted_again_with_the_same_commands_change_nothing() -> TestResult {
    let card: Arc<[Painted]> = Arc::from([leaf(1.0)]);
    let repainted: Arc<[Painted]> = Arc::from([leaf(1.0)]);
    let mut consumer = Consumer::default();
    assert!(
        consumer
            .show(&[leaf(0.0), component(7, 0.0, &card)])?
            .iter()
            .all(|kind| *kind == Kind::Insert)
    );
    assert!(
        consumer
            .show(&[leaf(0.0), component(7, 0.0, &repainted)])?
            .is_empty()
    );
    Ok(())
}
