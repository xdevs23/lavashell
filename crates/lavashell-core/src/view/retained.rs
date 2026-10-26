// SPDX-License-Identifier: GPL-3.0-only
use super::moving::MovingGroup;
use super::output::{GroupId, Painted, placed_at};
use super::paint::Inherited;
use super::{
    Affine, BTreeMap, BTreeSet, Constraints, Dp, FocusSource, Hit, HitClip, Key, Layout, Measuring,
    NodeId, Phase, Point, Rect, Scale, Size, TextMeasurer, Transform, View, Visibility,
    accessibility, collect_descendants, index_of, paint, scroll_followers,
};
use crate::style::ContentColors;
use crate::ui::{Identity, Part};
use std::sync::Arc;

#[derive(Default)]
pub(super) struct Retained {
    paintings: BTreeMap<NodeId, Painting>,
    layouts: BTreeMap<NodeId, Measured>,
}

#[derive(Clone)]
struct Measured {
    constraints: Constraints,
    /// The values of the nodes outside the component that are read by nodes inside the component, as measured then.
    reads: Vec<OutsideRead>,
    result: Size,
    nodes: Vec<(NodeId, Layout)>,
}

/// A value of a node outside a component that a node inside the component reads.
#[derive(Clone, Copy)]
enum OutsideRead {
    Size(NodeId, Size),
    NaturalWidth(NodeId, Dp),
}

/// The painting of a component, reused in later frames while `key` matches. The items, hits and geometry are in the
/// coordinates of the view from the frame in which the component was placed at `placed`.
struct Painting {
    key: PaintKey,
    items: Arc<[Painted]>,
    placed: Transform,
    /// The hits inside the component, with only the clips opened inside the component. Placing the painting again adds
    /// the clips around the component.
    hits: Vec<Hit>,
    /// The moving groups inside the component, to which the hits refer.
    moving: Vec<MovingGroup>,
    followers: Vec<scroll_followers::ScrollFollower>,
    geometry: Vec<(NodeId, Rect)>,
    fields: Vec<(NodeId, Scale)>,
    /// The accessibility geometry inside the component, with the visible part cut only by the clips opened inside the
    /// component.
    accessibility_geometry: Vec<(NodeId, accessibility::SemanticGeometry)>,
    modal: bool,
}

#[derive(PartialEq)]
struct InteractionKey {
    focus: Option<NodeId>,
    focus_source: Option<FocusSource>,
    /// For each pointer whose captured node or node under the pointer lies inside the component, those two nodes.
    pointers: Vec<(Option<NodeId>, Option<NodeId>)>,
    keyboard_press: Option<(NodeId, Key)>,
    window_focused: bool,
    modal_root: Option<NodeId>,
}

#[derive(PartialEq)]
struct PaintKey {
    /// The position of the component in the content of its parent, while the root node of the component has a
    /// transition. The transition animates from that position.
    offset: Option<Point>,
    /// The transform of the component in the view once every motion around it ends, and the laid-out origin of the
    /// component in the coordinates of the host, while the component contains a node shared in the whole view. Such a
    /// node animates in the coordinates of the host.
    placed: Option<(Affine, Point)>,
    sizes: Vec<(NodeId, Size)>,
    /// The hidden nodes. A node that becomes hidden moves the nodes next to it even when every size stays the same.
    hidden: Vec<NodeId>,
    interaction: InteractionKey,
    /// Where the layers of each node start. Offsets and positions move these starts also when no size changes.
    starts: Vec<(NodeId, Point)>,
    scrolls: Vec<(NodeId, Dp)>,
    leaving: Vec<NodeId>,
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
        self.paintings.retain(|key, painting| {
            !ids.contains(key) && !painting.geometry.iter().any(|(id, _)| ids.contains(id))
        });
        self.layouts.retain(|key, layout| {
            !ids.contains(key) && !layout.nodes.iter().any(|(id, _)| ids.contains(id))
        });
    }

    pub fn clear(&mut self) {
        self.paintings.clear();
        self.layouts.clear();
    }

    /// Drops the layout and the painting cached for the component `id`.
    fn remove_component(&mut self, id: NodeId) {
        let _painting_of_the_component = self.paintings.remove(&id);
        let _layout_of_the_component = self.layouts.remove(&id);
    }
}

impl<T: TextMeasurer> Measuring<'_, T> {
    /// Copies the layout cached for the component at `index` under `constraints`, and returns the size of the
    /// component. Returns `None` when a component in the subtree is dirty, when a value read from outside the
    /// component changed, or when the probe of a natural width read from outside the component reads a node that this
    /// pass has not measured yet.
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
            if let Some(i) = nodes.iter().position(|n| n.id == *id) {
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

    /// The value of the node at `source` that a node inside a component reads as `part`. A natural width that no node
    /// read in this layout was not read by the component either.
    fn outside_read(&self, source: usize, part: Part) -> Option<OutsideRead> {
        let (id, layout) = (self.nodes[source].id, &self.layout[source]);
        match part {
            Part::NaturalWidth => {
                (layout.natural_width).map(|width| OutsideRead::NaturalWidth(id, width))
            }
            _ => Some(OutsideRead::Size(id, layout.size)),
        }
    }

    /// Stops the tracking of layout and paint values for each hidden component, and drops the layout and the painting
    /// cached for the component. A hidden component lays out and paints nowhere, so a value that it read before would
    /// otherwise keep its scope dirty and request a frame in each turn.
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
        // The selection geometry of a focused field is not reactive state, so the component that contains the field
        // paints on every frame.
        let editing = (key.interaction.focus).is_some_and(|id| self.edits.contains_key(&id));
        let subtree_dirty = indices.iter().any(|i| {
            self.nodes[*i]
                .component
                .as_ref()
                .is_some_and(|c| c.scope.dirty().any())
        });
        let current =
            (self.retained.paintings.get(&id)).is_some_and(|painting| painting.key == key);
        if !editing && !subtree_dirty && current {
            self.stats.reused_components += 1;
            self.restore_painting(id, placed);
            self.place_painting(id, position, placed, inherited.clips);
            return;
        }
        let canvas = self.open_items();
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
            items: self.close_items(canvas).into(),
            placed,
            hits: collected.hits,
            moving: collected.moving.into_groups(),
            followers: (collected.followers.into_iter())
                .map(|follower| follower.in_component(id))
                .collect(),
            geometry: (indices.iter())
                .map(|i| (self.nodes[*i].id, self.layout[*i].rect))
                .collect(),
            fields: (indices.iter())
                .filter_map(|i| {
                    let id = self.nodes[*i].id;
                    self.text_positions.get(&id).map(|field| (id, field.scale))
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
        let shared = (indices.iter()).any(|i| self.nodes[*i].identity == Identity::InView);
        PaintKey {
            offset: (self.nodes[index].transition)
                .map(|_| Point::new(position.x - origin.x, position.y - origin.y)),
            placed: shared.then(|| {
                let laid_out = (inherited.laid_out.offset(position.x, position.y))
                    .offset(self.viewport_origin.x, self.viewport_origin.y);
                (self.moving.resting().then(placed).affine(), laid_out)
            }),
            sizes: indices
                .iter()
                .map(|i| (self.nodes[*i].id, self.layout[*i].size))
                .collect(),
            hidden: (indices.iter())
                .filter(|i| self.layout[**i].visibility == Visibility::Hidden)
                .map(|i| self.nodes[*i].id)
                .collect(),
            starts: (indices.iter())
                .flat_map(|i| {
                    let id = self.nodes[*i].id;
                    (self.layout[*i].starts.iter()).map(move |start| (id, *start))
                })
                .collect(),
            interaction: InteractionKey {
                focus: local(self.focus),
                focus_source: local(self.focus).and(self.focus_source),
                pointers: (self.pointers.values())
                    .map(|pointer| (local(pointer.capture()), local(pointer.over)))
                    .filter(|pointer| *pointer != (None, None))
                    .collect(),
                keyboard_press: self.keyboard_press.filter(|(id, _)| ids.contains(id)),
                window_focused: self.window_focused,
                modal_root: self.modal_root,
            },
            scrolls: self
                .scrolls
                .iter()
                .filter(|(id, _)| ids.contains(id))
                .map(|(id, y)| (*id, *y))
                .collect(),
            leaving: self
                .sibling_orders
                .leaving_under(ids.iter().copied(), |id| self.transitions.is_leaving(id)),
            content_colors: inherited.content_colors.clone(),
            source: inherited.source,
            flattened: self.canvas.flattened(),
        }
    }

    fn restore_painting(&mut self, id: NodeId, placed: Transform) {
        let Some(painting) = self.retained.paintings.get_mut(&id) else {
            return;
        };
        let moved = painting.moved(placed);
        for hit in (painting.hits.iter_mut()).filter(|hit| hit.input.is_some()) {
            hit.input = self.nodes.iter().position(|n| n.id == hit.id);
        }
        for (node, rect) in &painting.geometry {
            if let Some(i) = self.nodes.iter().position(|n| n.id == *node) {
                self.layout[i].rect = moved.rect(*rect);
            }
        }
        for (node, scale) in &painting.fields {
            if let Some(field) = self.text_positions.get_mut(node) {
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
            self.hits.clear();
        }
        // The hits and clips recorded by the painting outside its moving groups have no group. They belong to the
        // current group of the view, in which the painting is placed now.
        let around = self.moving.current;
        self.moving.place(&painting.moving, moved);
        self.followers.extend(painting.followers.iter().cloned());
        for hit in &painting.hits {
            let inner = (hit.clips.iter()).map(|clip| {
                let clip = clip.mapped(moved);
                clip.in_group(clip.group.or(around))
            });
            self.hits.push(Hit {
                rect: moved.rect(hit.rect),
                clips: clips.iter().copied().chain(inner).collect(),
                group: hit.group.or(around),
                ..hit.clone()
            });
        }
        for (node, geometry) in &painting.accessibility_geometry {
            let _geometry_of_the_previous_frame =
                (self.accessibility_geometry).insert(*node, geometry.placed(moved, clips));
        }
        let items = painting.items.clone();
        self.canvas.nest(Painted::Group {
            id: GroupId::Component(id),
            group: placed_at(position),
            items,
        });
    }
}
