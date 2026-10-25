// SPDX-License-Identifier: GPL-3.0-only
use super::test_changes::insert;
use super::*;
use crate::animation::{Animatable, MotionSpec, Playing, Program, Timing, Tween, Vector};
use crate::effects::{Backdrop, blur_reach};
use crate::geometry::{Color, Point, Size, Viewport};
use crate::output::finest_step;
use crate::scene::Affine;
use crate::test_error::TestResult;
use crate::units::{Density, Dp, DpExt};
use std::time::Duration;

/// A viewport in which one device pixel is 1 dp. Damage grows every footprint by one device pixel, which is 1 dp here.
const VIEWPORT: Viewport = Viewport::new(Size::new(Dp::new(200.0), Dp::new(100.0)), Density::ONE);

/// The content of a leaf that fills `rect`.
fn fill_content(rect: Rect) -> Content {
    let fill = DrawCommand::Fill {
        shape: rect.into(),
        brush: Color::WHITE.into(),
    };
    Content::Packet(Packet::new(Arc::from([fill])))
}

/// A leaf that fills `rect`, whose footprint is `rect`.
fn fill(rect: Rect) -> Node {
    Node::Leaf(fill_content(rect))
}

fn moved_by(transform: Affine) -> Node {
    Node::Group(Group {
        transform: Slot::Value(transform),
        ..Group::default()
    })
}

struct Shown {
    tree: SceneTree,
    presented: DamageTracker,
}

impl Shown {
    fn apply(&mut self, changes: impl IntoIterator<Item = Change>) -> Result<(), ApplyError> {
        self.tree.show(SceneUpdate::from_iter(changes))
    }

    fn damage(&mut self, now: Duration) -> Option<Rect> {
        self.presented.damage(self.tree.at(now), VIEWPORT)
    }
}

fn presented(changes: impl IntoIterator<Item = Change>) -> Result<Shown, ApplyError> {
    let mut shown = Shown {
        tree: SceneTree::default(),
        presented: DamageTracker::default(),
    };
    shown.apply(changes)?;
    assert_eq!(
        shown.damage(Duration::ZERO),
        Some(Rect::from_size(VIEWPORT.size))
    );
    assert_eq!(shown.damage(Duration::ZERO), None);
    Ok(shown)
}

#[test]
fn a_moved_group_damages_its_old_and_new_mapped_bounds_and_a_removed_one_its_last() -> TestResult {
    let at = |x: Dp| Affine::translate(Point::new(x, 0.dp()));
    let mut tree = presented([
        insert(1, None, 0, moved_by(at(10.dp()))),
        insert(
            2,
            Some(1),
            0,
            fill(Rect::new(0.dp(), 0.dp(), 20.dp(), 20.dp())),
        ),
    ])?;
    let moved = Change::SetSlot {
        id: SceneNodeId(1),
        slot: SlotChange::Transform(Slot::Value(at(50.dp()))),
    };
    tree.apply([moved])?;
    let both = Rect::new(9.dp(), 0.dp(), 62.dp(), 21.dp());
    assert_eq!(tree.damage(Duration::ZERO), Some(both));
    assert_eq!(tree.damage(Duration::ZERO), None);
    tree.apply([Change::Remove { id: SceneNodeId(1) }])?;
    let last = Rect::new(49.dp(), 0.dp(), 22.dp(), 21.dp());
    assert_eq!(tree.damage(Duration::ZERO), Some(last));
    Ok(())
}

#[test]
fn a_rotated_packet_damages_its_mapped_footprint_only() -> TestResult {
    let turned = Affine {
        linear: [0.0, -1.0, 1.0, 0.0],
        translation: Point::new(100.dp(), 10.dp()),
    };
    let rect = Rect::new(0.dp(), 0.dp(), 40.dp(), 20.dp());
    let mut tree = presented([
        insert(1, None, 0, moved_by(turned)),
        insert(2, Some(1), 0, fill(rect)),
    ])?;
    let repainted = Change::SetContent {
        id: SceneNodeId(2),
        content: fill_content(rect),
    };
    tree.apply([repainted])?;
    let mapped = Rect::new(79.dp(), 9.dp(), 22.dp(), 42.dp());
    assert_eq!(tree.damage(Duration::ZERO), Some(mapped));
    Ok(())
}

#[test]
fn a_slot_motion_damages_its_subtree_while_it_moves() -> TestResult {
    let fading = Node::Group(Group {
        opacity: Slot::Motion(Box::new(Playing::new(
            1.0,
            Program::Target {
                from: Vector([0.0]),
                to: Vector([1.0]),
                velocity: Vector([0.0]),
                spec: MotionSpec::Tween(Tween::new(Duration::from_millis(100))),
                step: f64::visible_step(VIEWPORT.pixel()),
            },
            Timing {
                start: Duration::ZERO,
                paused: None,
                offset: Duration::ZERO,
                rate: 1.0,
            },
        )?)),
        ..Group::default()
    });
    let rect = Rect::new(0.dp(), 0.dp(), 20.dp(), 20.dp());
    let mut tree = presented([
        insert(1, None, 0, fading),
        insert(2, Some(1), 0, fill(rect)),
    ])?;
    let leaf = Rect::new(0.dp(), 0.dp(), 21.dp(), 21.dp());
    assert_eq!(tree.damage(Duration::from_millis(50)), Some(leaf));
    assert_eq!(tree.damage(Duration::from_millis(100)), Some(leaf));
    assert_eq!(tree.damage(Duration::from_millis(150)), None);
    Ok(())
}

#[test]
fn a_tree_presented_from_nothing_damages_the_whole_viewport_once() -> TestResult {
    let rect = Rect::new(0.dp(), 0.dp(), 20.dp(), 20.dp());
    let mut tree = presented([insert(1, None, 0, fill(rect))])?;
    tree.presented = DamageTracker::default();
    assert_eq!(
        tree.damage(Duration::ZERO),
        Some(Rect::from_size(VIEWPORT.size))
    );
    assert_eq!(tree.damage(Duration::ZERO), None);
    Ok(())
}

#[test]
fn reordering_siblings_damages_only_the_moved_node() -> TestResult {
    let mut tree = presented([
        insert(
            1,
            None,
            0,
            fill(Rect::new(0.dp(), 0.dp(), 20.dp(), 20.dp())),
        ),
        insert(
            2,
            None,
            1,
            fill(Rect::new(100.dp(), 10.dp(), 20.dp(), 20.dp())),
        ),
    ])?;
    let reordered = Change::Move {
        id: SceneNodeId(2),
        parent: None,
        index: 0,
    };
    tree.apply([reordered])?;
    let moved = Rect::new(99.dp(), 9.dp(), 22.dp(), 22.dp());
    assert_eq!(tree.damage(Duration::ZERO), Some(moved));
    Ok(())
}

#[test]
fn removing_a_graft_damages_what_the_grafted_namespace_showed() -> TestResult {
    let mut tree = presented([])?;
    let grafted = tree.tree.create_namespace();
    let rect = Rect::new(0.dp(), 0.dp(), 20.dp(), 20.dp());
    tree.tree.apply(
        grafted,
        SceneUpdate::from_iter([insert(1, None, 0, fill(rect))]),
    )?;
    tree.apply([insert(1, None, 0, Node::Leaf(Content::Graft(grafted)))])?;
    let shown = Rect::new(0.dp(), 0.dp(), 21.dp(), 21.dp());
    assert_eq!(tree.damage(Duration::ZERO), Some(shown));
    tree.apply([Change::Remove { id: SceneNodeId(1) }])?;
    assert_eq!(tree.damage(Duration::ZERO), Some(shown));
    Ok(())
}

#[test]
fn a_leaf_that_shows_an_earlier_packet_again_damages_what_it_paints() -> TestResult {
    let rect = Rect::new(10.dp(), 10.dp(), 20.dp(), 20.dp());
    let shown = Rect::new(9.dp(), 9.dp(), 22.dp(), 22.dp());
    let (first, second) = (fill_content(rect), fill_content(rect));
    let mut tree = presented([insert(1, None, 0, Node::Leaf(first.clone()))])?;
    for content in [second, first] {
        tree.apply([Change::SetContent {
            id: SceneNodeId(1),
            content,
        }])?;
        assert_eq!(tree.damage(Duration::ZERO), Some(shown));
    }
    Ok(())
}

#[test]
fn a_group_or_a_graft_in_place_of_a_leaf_damages_what_the_leaf_showed() -> TestResult {
    let rect = Rect::new(10.dp(), 10.dp(), 20.dp(), 20.dp());
    let shown = Rect::new(9.dp(), 9.dp(), 22.dp(), 22.dp());
    let mut tree = presented([insert(1, None, 0, fill(rect))])?;
    tree.apply([
        Change::Remove { id: SceneNodeId(1) },
        insert(1, None, 0, moved_by(Affine::IDENTITY)),
    ])?;
    assert_eq!(tree.damage(Duration::ZERO), Some(shown));
    let mut tree = presented([insert(1, None, 0, fill(rect))])?;
    let empty = tree.tree.create_namespace();
    tree.apply([Change::SetContent {
        id: SceneNodeId(1),
        content: Content::Graft(empty),
    }])?;
    assert_eq!(tree.damage(Duration::ZERO), Some(shown));
    Ok(())
}

#[test]
fn a_change_under_a_backdrop_damages_the_reader_and_what_it_reads() -> TestResult {
    let glass = Rect::new(30.dp(), 0.dp(), 30.dp(), 30.dp());
    let blur = DrawCommand::Backdrop {
        shape: glass.into(),
        effect: Backdrop::blur(2.dp())?,
    };
    let reader = Node::Leaf(Content::Packet(Packet::new(Arc::from([blur]))));
    let below = Rect::new(0.dp(), 40.dp(), 24.dp(), 20.dp());
    let mut tree = presented([insert(1, None, 0, fill(below)), insert(2, None, 1, reader)])?;
    let repainted = Change::SetContent {
        id: SceneNodeId(1),
        content: fill_content(Rect::new(0.dp(), 0.dp(), 24.dp(), 20.dp())),
    };
    tree.apply([repainted])?;
    // The damage spreads to the backdrop leaf, because the repainted fill lies in the area read by its blur. That
    // area extends `2 dp * blur_reach` and one device pixel past the right side of the glass. Damage is rounded out to
    // whole dp.
    let width = (60.dp() + 2.dp() * blur_reach(finest_step()) + VIEWPORT.pixel()).ceil();
    let damage = Rect::new(0.dp(), 0.dp(), width, 61.dp());
    assert_eq!(tree.damage(Duration::ZERO), Some(damage));
    Ok(())
}
