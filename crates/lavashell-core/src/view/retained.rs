// SPDX-License-Identifier: GPL-3.0-only
use super::crossing::Crossed;
use super::moving::MovingGroup;
use super::output::{GroupId, Painted, placed_at};
use super::paint::{Above, Detached, Inherited};
use super::{Hit, HitClip, TextMeasurer, TextTarget, View, accessibility, paint, scroll_followers};
use crate::geometry::{Constraints, Point, Rect, Scale, Size};
use crate::reactive::Phase;
use crate::scene::{Affine, Transform};
use crate::style::ContentColors;
use crate::ui::{Identity, NodeId, Part};
use crate::units::Dp;
use crate::view::component::{collect_descendants, index_of};
use crate::view::focus::{FocusSource, FocusedNode};
use crate::view::layout::{Layout, Measuring, Visibility};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

#[derive(Default)]
pub(super) struct Retained {
    paintings: BTreeMap<NodeId, Painting>,
    layouts: BTreeMap<NodeId, Measured>,
}

#[derive(Clone)]
struct Measured {
    constraints: Constraints,
    reads: Vec<OutsideRead>,
    result: Size,
    nodes: Vec<(NodeId, Layout)>,
}

#[derive(Clone, Copy)]
enum OutsideRead {
    Size(NodeId, Size),
    NaturalWidth(NodeId, Dp),
}

struct Painting {
    key: PaintKey,
    items: Arc<[Painted]>,
    placed: Transform,
    hits: Vec<Hit>,
    moving: Vec<MovingGroup>,
    followers: Vec<scroll_followers::ScrollFollower>,
    above: Vec<Above>,
    flown: BTreeMap<NodeId, Detached>,
    geometry: Vec<(NodeId, Rect)>,
    fields: Vec<(NodeId, Scale)>,
    accessibility_geometry: Vec<(NodeId, accessibility::SemanticGeometry)>,
    modal: bool,
}

#[derive(PartialEq)]
struct InteractionKey {
    focused: Option<FocusedNode>,
    focus_source: Option<FocusSource>,
    pointers: Vec<(Option<NodeId>, Option<NodeId>)>,
    window_focused: bool,
    modal_root: Option<NodeId>,
}

#[derive(PartialEq)]
struct PaintKey {
    offset: Option<Point>,
    placed: Option<(Affine, Point)>,
    sizes: Vec<(NodeId, Size)>,
    hidden: Vec<NodeId>,
    selectable: Vec<NodeId>,
    interaction: InteractionKey,
    starts: Vec<(NodeId, Point)>,
    scrolls: Vec<(NodeId, Dp)>,
    leaving: Vec<NodeId>,
    crossed: Vec<(NodeId, Crossed)>,
    content_colors: ContentColors,
    source: paint::Source,
    flattened: bool,
}

impl Painting {
    fn moved(&self, placed: Transform) -> Transform {
        if self.placed == placed {
            Transform::IDENTITY
        } else {
            placed.then(self.placed.inverse())
        }
    }
}

impl Retained {
    pub fn remove(&mut self, ids: &BTreeSet<NodeId>) {
        self.forget_paintings_around(ids);
        self.layouts.retain(|key, layout| {
            !ids.contains(key) && !layout.nodes.iter().any(|(id, _)| ids.contains(id))
        });
    }

    pub fn forget_paintings_around(&mut self, ids: &BTreeSet<NodeId>) {
        self.paintings.retain(|key, painting| {
            !ids.contains(key) && !painting.geometry.iter().any(|(id, _)| ids.contains(id))
        });
    }

    pub fn clear(&mut self) {
        self.paintings.clear();
        self.layouts.clear();
    }

    fn remove_component(&mut self, id: NodeId) {
        let _painting_of_the_component = self.paintings.remove(&id);
        let _layout_of_the_component = self.layouts.remove(&id);
    }
}

impl<T: TextMeasurer> Measuring<'_, T> {
    pub(super) fn reuse_layout(
        &mut self,
        cache: &Retained,
        index: usize,
        constraints: Constraints,
    ) -> Option<Size> {
        let nodes = self.nodes;
        let mut indices = BTreeSet::new();
        collect_descendants(nodes, index, &mut indices);
        let layout_dirty = indices.iter().any(|i| {
            nodes[*i].component.as_ref().is_some_and(|c| {
                c.scope.dirty().contains(Phase::Layout)
                    || c.scope.dirty().contains(Phase::Composition)
            })
        });
        if layout_dirty {
            return None;
        }
        let id = nodes[index].id;
        let cached = (cache.layouts.get(&id)).filter(|c| c.constraints == constraints)?;
        if !cached.reads.iter().all(|&read| self.reads_the_same(read)) {
            return None;
        }
        for (id, layout) in &cached.nodes {
            if let Some(i) = index_of(nodes, *id) {
                self.layout[i] = layout.clone();
                self.filled[i] = true;
            }
        }
        self.reused += 1;
        Some(cached.result)
    }

    fn reads_the_same(&mut self, read: OutsideRead) -> bool {
        match read {
            OutsideRead::Size(id, size) => {
                index_of(self.nodes, id).is_some_and(|i| self.layout[i].size == size)
            }
            OutsideRead::NaturalWidth(id, width) => {
                let graph = self.graph;
                index_of(self.nodes, id).is_some_and(|i| {
                    graph.read_by_probe(i).all(|read| self.filled[read])
                        && self.natural_width_at(i) == width
                })
            }
        }
    }
}

impl View {
    pub(super) fn record_layouts(&mut self, components: Vec<(usize, Constraints)>) {
        for (index, constraints) in components {
            let mut indices = BTreeSet::new();
            collect_descendants(&self.nodes, index, &mut indices);
            let nodes = indices
                .iter()
                .map(|i| (self.nodes[*i].id, self.layout[*i].clone()))
                .collect();
            let reads = (self.graph.read_into(&indices))
                .filter_map(|(source, part)| self.outside_read(source, part))
                .collect();
            let _layout_of_the_previous_frame = self.retained.layouts.insert(
                self.nodes[index].id,
                Measured {
                    constraints,
                    reads,
                    result: self.layout[index].size,
                    nodes,
                },
            );
            self.stats.measured_components += 1;
        }
    }

    fn outside_read(&self, source: usize, part: Part) -> Option<OutsideRead> {
        let (id, layout) = (self.nodes[source].id, &self.layout[source]);
        match part {
            Part::NaturalWidth => {
                (layout.natural_width).map(|width| OutsideRead::NaturalWidth(id, width))
            }
            _ => Some(OutsideRead::Size(id, layout.size)),
        }
    }

    pub(super) fn release_hidden_components(&mut self) {
        for (node, layout) in self.nodes.iter().zip(&self.layout) {
            if layout.visibility == Visibility::Hidden
                && let Some(component) = &node.component
            {
                component.scope.forget(Phase::Layout);
                component.scope.forget(Phase::Paint);
                self.retained.remove_component(node.id);
            }
        }
    }

    pub(super) fn paint(
        &mut self,
        index: usize,
        position: Point,
        origin: Point,
        transform: Transform,
        inherited: Inherited,
        text: &mut impl TextMeasurer,
    ) {
        let Some(component) = self.nodes[index].component.clone() else {
            self.paint_inner(index, position, origin, transform, inherited, text);
            return;
        };
        let id = self.nodes[index].id;
        let placed = transform.then(Transform::translation(position));
        let mut indices = BTreeSet::new();
        collect_descendants(&self.nodes, index, &mut indices);
        let key = self.paint_key(index, &indices, position, origin, placed, &inherited);
        let editing = (key.interaction.focused)
            .is_some_and(|focused| self.editing.edits.contains_key(&focused.node));
        let subtree_dirty = indices.iter().any(|i| self.nodes[*i].component_is_dirty());
        let current =
            (self.retained.paintings.get(&id)).is_some_and(|painting| painting.key == key);
        if !editing && !subtree_dirty && current {
            self.stats.reused_components += 1;
            self.restore_painting(id, placed);
            self.place_painting(id, position, placed, inherited.clips);
            return;
        }
        let canvas = self.painting.open_items();
        let outer = self.isolate();
        let inside = Inherited {
            clips: &[],
            laid_out: inherited.laid_out.offset(position.x, position.y),
            ..inherited
        };
        let parent_origin = Point::new(origin.x - position.x, origin.y - position.y);
        component.scope.track(Phase::Paint, || {
            self.paint_inner(index, Point::default(), parent_origin, placed, inside, text);
        });
        let collected = self.restore(outer);
        let painting = Painting {
            key,
            items: self.painting.close_items(canvas).into(),
            placed,
            hits: collected.hits,
            moving: collected.moving.into_groups(),
            followers: (collected.followers.into_iter())
                .map(|follower| follower.in_component(id))
                .collect(),
            above: collected.above,
            flown: collected.flown,
            geometry: (indices.iter())
                .map(|i| (self.nodes[*i].id, self.layout[*i].rect))
                .collect(),
            fields: (indices.iter())
                .filter_map(|i| {
                    let id = self.nodes[*i].id;
                    self.editing
                        .positions
                        .get(&id)
                        .map(|field| (id, field.scale))
                })
                .collect(),
            accessibility_geometry: collected.geometry.into_iter().collect(),
            modal: indices.iter().any(|&i| self.is_modal(i)),
        };
        let _painting_of_the_previous_frame = self.retained.paintings.insert(id, painting);
        self.stats.painted_components += 1;
        self.place_painting(id, position, placed, inherited.clips);
    }

    fn paint_key(
        &self,
        index: usize,
        indices: &BTreeSet<usize>,
        position: Point,
        origin: Point,
        placed: Transform,
        inherited: &Inherited,
    ) -> PaintKey {
        let ids = indices
            .iter()
            .map(|i| self.nodes[*i].id)
            .collect::<BTreeSet<_>>();
        let local = |id: Option<NodeId>| id.filter(|id| ids.contains(id));
        let focused =
            (self.routing.focus.focused_node()).filter(|focused| ids.contains(&focused.node));
        let shared =
            (indices.iter()).any(|i| matches!(self.nodes[*i].identity, Identity::Shared { .. }));
        PaintKey {
            offset: (self.nodes[index].transition)
                .map(|_| Point::new(position.x - origin.x, position.y - origin.y)),
            placed: shared.then(|| {
                let laid_out = (inherited.laid_out.offset(position.x, position.y))
                    .offset(self.viewport_origin.x, self.viewport_origin.y);
                (self.routing.moving.at_rest(placed).affine(), laid_out)
            }),
            sizes: indices
                .iter()
                .map(|i| (self.nodes[*i].id, self.layout[*i].size))
                .collect(),
            hidden: (indices.iter())
                .filter(|i| self.layout[**i].visibility == Visibility::Hidden)
                .map(|i| self.nodes[*i].id)
                .collect(),
            selectable: (indices.iter())
                .filter(|i| self.text_selectable(**i))
                .map(|i| self.nodes[*i].id)
                .collect(),
            starts: (indices.iter())
                .flat_map(|i| {
                    let id = self.nodes[*i].id;
                    (self.layout[*i].starts.iter()).map(move |start| (id, *start))
                })
                .collect(),
            interaction: InteractionKey {
                focused,
                focus_source: focused.and(self.routing.focus.source()),
                pointers: (self.routing.pointers.values())
                    .map(|pointer| (local(pointer.capture()), local(pointer.over)))
                    .filter(|pointer| *pointer != (None, None))
                    .collect(),
                window_focused: self.window_focused,
                modal_root: self.routing.modal_root,
            },
            scrolls: self
                .scrolls
                .iter()
                .filter(|(id, _)| ids.contains(id))
                .map(|(id, y)| (*id, *y))
                .collect(),
            leaving: self
                .painting
                .sibling_orders
                .leaving_under(ids.iter().copied(), |id| self.transitions.is_leaving(id)),
            crossed: (indices.iter())
                .filter_map(|i| Some((self.nodes[*i].id, self.crossings.crossed(*i)?.clone())))
                .collect(),
            content_colors: inherited.content_colors.clone(),
            source: inherited.source,
            flattened: self.painting.canvas.flattened(),
        }
    }

    fn restore_painting(&mut self, id: NodeId, placed: Transform) {
        let Some(painting) = self.retained.paintings.get_mut(&id) else {
            return;
        };
        let moved = painting.moved(placed);
        for hit in &mut painting.hits {
            let index = index_of(&self.nodes, hit.id);
            if let Some(target) = hit.input {
                hit.input = index.map(|index| TextTarget { index, ..target });
            } else if let Some(index) = index {
                hit.use_handlers_of(&self.nodes[index]);
            }
        }
        for (node, rect) in &painting.geometry {
            if let Some(i) = index_of(&self.nodes, *node) {
                self.layout[i].rect = moved.rect(*rect);
            }
        }
        for (node, scale) in &painting.fields {
            if let Some(field) = self.editing.positions.get_mut(node) {
                field.scale = Scale {
                    x: scale.x * moved.scale.x,
                    y: scale.y * moved.scale.y,
                };
            }
        }
    }

    fn place_painting(
        &mut self,
        id: NodeId,
        position: Point,
        placed: Transform,
        clips: &[HitClip],
    ) {
        let Some(painting) = self.retained.paintings.get(&id) else {
            return;
        };
        let moved = painting.moved(placed);
        if painting.modal {
            self.routing.hits.clear();
        }
        let around = self.routing.moving.current;
        self.routing.moving.place(&painting.moving, moved);
        self.followers.extend(painting.followers.iter().cloned());
        (self.painting.above).extend(painting.above.iter().map(|above| above.moved_by(moved)));
        (self.painting.flown)
            .extend((painting.flown.iter()).map(|(id, flown)| (*id, flown.moved_by(moved))));
        for hit in &painting.hits {
            let inner = (hit.clips.iter()).map(|clip| {
                let clip = clip.mapped(moved);
                clip.in_group(clip.group.or(around))
            });
            self.routing.hits.push(Hit {
                rect: moved.rect(hit.rect),
                input: hit.input.map(|target| target.mapped(moved)),
                clips: clips.iter().copied().chain(inner).collect(),
                group: hit.group.or(around),
                ..hit.clone()
            });
        }
        for (node, geometry) in &painting.accessibility_geometry {
            let _geometry_of_the_previous_frame =
                (self.accessibility.geometry).insert(*node, geometry.placed(moved, clips));
        }
        let items = painting.items.clone();
        self.painting.canvas.nest(Painted::Group {
            id: GroupId::Component(id),
            group: placed_at(position),
            items,
        });
    }
}
