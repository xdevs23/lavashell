// SPDX-License-Identifier: GPL-3.0-only
use super::{
    GroupValues, NamespaceId, NodeKey, PacketGeneration, PaintVisitor, PlacedLeaf, Presented, Rect,
    SceneTree,
};
use crate::geometry::Viewport;
use crate::retained::Extent;
use crate::scene::Affine;
use crate::units::Dp;
use std::collections::HashMap;

#[derive(Default)]
pub struct DamageTracker {
    viewport: Option<Viewport>,
    records: HashMap<NodeKey, Record>,
    /// The number of the last presentation. The record of every node shown in that presentation has the same number.
    presentation: u64,
}

struct Record {
    /// The number of the insert or move that last put the node at its place in paint order. The tree counts its
    /// inserts and moves from 1.
    placed: Option<u64>,
    painted: PaintedNode,
    presentation: u64,
}

#[derive(PartialEq)]
enum PaintedNode {
    Group(GroupValues),
    Graft(NamespaceId),
    /// `output` is the visible part of the area painted by the leaf, in root coordinates and grown by one device
    /// pixel. `backdrop` is the part of the area read by a backdrop effect of the leaf that lies inside the viewport.
    Leaf {
        generation: PacketGeneration,
        output: Option<Rect>,
        backdrop: Option<Rect>,
    },
}

#[derive(Clone, Copy)]
struct Enclosing {
    changed: bool,
    visible: Option<Rect>,
}

/// The walk over one presentation of a tree, which compares every node with its record from the presentation before.
struct Pass<'a> {
    tree: &'a SceneTree,
    records: &'a mut HashMap<NodeKey, Record>,
    presentation: u64,
    viewport: Rect,
    /// The size of one device pixel. Antialiased edges and sampling paint up to this far outside the exact geometry.
    pixel: Dp,
    scope: Enclosing,
    damage: Option<Rect>,
    readers: Vec<(Rect, Rect)>,
}

impl DamageTracker {
    /// Forgets every record, so the next presentation damages the whole viewport. Call it when the target has lost
    /// its pixels.
    pub fn invalidate(&mut self) {
        *self = Self::default();
    }

    /// The area of `viewport` that changed since the last presentation, rounded out to whole dp.
    pub fn damage(&mut self, scene: Presented<'_>, viewport: Viewport) -> Option<Rect> {
        self.presentation += 1;
        let presentation = self.presentation;
        let area = Rect::from_size(viewport.size);
        let mut pass = Pass {
            tree: scene.tree,
            records: &mut self.records,
            presentation,
            viewport: area,
            pixel: viewport.pixel(),
            scope: Enclosing {
                changed: false,
                visible: Some(area),
            },
            damage: (self.viewport != Some(viewport)).then_some(area),
            readers: Vec::new(),
        };
        scene.paint_order().walk(&mut pass);
        let Pass {
            mut damage,
            readers,
            ..
        } = pass;
        self.records.retain(|_, record| {
            if record.presentation != presentation {
                record
                    .output()
                    .into_iter()
                    .for_each(|gone| add(&mut damage, gone));
            }
            record.presentation == presentation
        });
        spread_to_readers(&mut damage, &readers);
        self.viewport = Some(viewport);
        damage.and_then(|r| whole(r, area))
    }
}

impl Pass<'_> {
    /// Whether `key` paints differently from the last presentation, or a group around it changed.
    fn changed(&self, key: NodeKey, painted: &PaintedNode) -> bool {
        self.scope.changed
            || (self.records.get(&key))
                .is_none_or(|old| old.placed != self.tree.placed(key) || old.painted != *painted)
    }

    /// Records how `key` paints. When `changed`, adds the old and the new output of `key` to the damage.
    fn visit(&mut self, key: NodeKey, painted: PaintedNode, changed: bool) {
        let record = Record {
            placed: self.tree.placed(key),
            painted,
            presentation: self.presentation,
        };
        let output = record.output();
        let shown = (self.records.insert(key, record)).and_then(|old| old.output());
        if changed {
            (shown.into_iter())
                .chain(output)
                .for_each(|r| add(&mut self.damage, r));
        }
    }
}

impl<'p> PaintVisitor<'p> for Pass<'_> {
    type Saved = Enclosing;

    fn group(&mut self, key: NodeKey, values: GroupValues, world: Affine) -> Enclosing {
        let visible = match values.clip {
            Some(clip) => (self.scope.visible)
                .and_then(|visible| Extent::from(clip.rect).mapped(world).within(visible)),
            None => self.scope.visible,
        };
        let painted = PaintedNode::Group(values);
        let changed = self.changed(key, &painted);
        self.visit(key, painted, changed);
        std::mem::replace(&mut self.scope, Enclosing { changed, visible })
    }

    fn graft(&mut self, key: NodeKey, namespace: NamespaceId) -> Enclosing {
        let painted = PaintedNode::Graft(namespace);
        let changed = self.changed(key, &painted);
        self.visit(key, painted, changed);
        let visible = self.scope.visible;
        std::mem::replace(&mut self.scope, Enclosing { changed, visible })
    }

    fn leaf(&mut self, leaf: PlacedLeaf<'p>) {
        let footprint = leaf.packet.footprint();
        let in_root = |extent: Extent| extent.mapped(leaf.world).outset(self.pixel);
        let output = (footprint.output)
            .zip(self.scope.visible)
            .and_then(|(painted, visible)| in_root(painted).within(visible));
        let backdrop = (footprint.backdrop).and_then(|read| in_root(read).within(self.viewport));
        self.readers.extend(output.zip(backdrop));
        let painted = PaintedNode::Leaf {
            generation: leaf.packet.generation(),
            output,
            backdrop,
        };
        let changed = self.changed(leaf.key, &painted);
        self.visit(leaf.key, painted, changed);
    }

    fn exit(&mut self, saved: Enclosing) {
        self.scope = saved;
    }
}

impl Record {
    fn output(&self) -> Option<Rect> {
        match self.painted {
            PaintedNode::Leaf { output, .. } => output,
            PaintedNode::Group(_) | PaintedNode::Graft(_) => None,
        }
    }
}

fn add(damage: &mut Option<Rect>, r: Rect) {
    *damage = Some(damage.map_or(r, |old| old.union(r)));
}

/// Adds to the damage the output and the read area of every backdrop leaf whose output or read area overlaps the
/// damage, until the damage stops growing.
fn spread_to_readers(damage: &mut Option<Rect>, readers: &[(Rect, Rect)]) {
    for _ in 0..readers.len().max(1) {
        let before = *damage;
        for (output, read) in readers {
            let overlaps =
                |area: Rect| damage.is_some_and(|dirty| dirty.intersection(area).is_some());
            if overlaps(*read) || overlaps(*output) {
                add(damage, *read);
                add(damage, *output);
            }
        }
        if before == *damage {
            break;
        }
    }
}

fn whole(damage: Rect, viewport: Rect) -> Option<Rect> {
    let (x, y) = (damage.x.floor(), damage.y.floor());
    let right = (damage.x + damage.width).ceil();
    let bottom = (damage.y + damage.height).ceil();
    Rect::new(x, y, right - x, bottom - y).intersection(viewport)
}
