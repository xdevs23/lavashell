// SPDX-License-Identifier: GPL-3.0-only
use super::group::OpenGroup;
use super::{Inherited, NodeId, Point, Rect, Source, Surfaces, TextMeasurer, Transform, View};
use crate::scene::tree::{PresenceMap, PresenceVectors, Slot, TransformMotion};
use crate::view::collected::Collected;
use crate::view::laid_out::LaidOutTree;
use crate::view::output::GroupId;
use crate::view::transition::{AnimatedVisual, BoundsMotion, Visual, VisualVectors};

pub(super) struct OpenedPresence {
    pub local: Transform,
    /// Where the surfaces of the node, such as its background and border, paint at its animated bounds, before the
    /// presence transform of the node applies.
    pub surfaces: Surfaces,
}

/// The coordinates in which the transition of a node animates its bounds: relative to `origin`,
/// and mapped by `outer` into the coordinates in which the node paints.
pub(super) struct TransitionSpace {
    pub origin: Point,
    pub outer: Transform,
}

impl TransitionSpace {
    /// The space of a node with an identity in the whole view. `resting` maps the paint coordinates into the view once
    /// every motion around the node has ended, and `laid_out` is the origin of the paint coordinates in the view as
    /// laid out. The bounds animate in the coordinates of the host, in which the viewport lies at `viewport`.
    pub fn in_view(resting: Transform, laid_out: Point, viewport: Point) -> Self {
        Self {
            origin: Point::new(-laid_out.x - viewport.x, -laid_out.y - viewport.y),
            outer: resting.inverse().then(Transform::translation(laid_out)),
        }
    }

    /// The space of a node with an identity among the children of its parent: coordinates relative to `origin`, the
    /// content origin of the parent.
    pub fn in_parent(origin: Point) -> Self {
        Self {
            origin,
            outer: Transform::IDENTITY,
        }
    }

    /// The bounds `rect`, given where the node paints, in this space.
    pub fn relative(&self, rect: Rect) -> Rect {
        Rect::new(
            rect.x - self.origin.x,
            rect.y - self.origin.y,
            rect.width,
            rect.height,
        )
    }

    fn presence(
        &self,
        visual: &Visual,
        motion: BoundsMotion,
        bounds: Rect,
    ) -> (PresenceMap, Surfaces) {
        let (surfaces, stretch) = Surfaces::new(motion, bounds, visual.rect, self.origin);
        let map = PresenceMap::new(self.outer, stretch, surfaces.rect(bounds));
        (map, surfaces)
    }
}

fn position(visual: &Visual) -> Point {
    Point::new(visual.rect.x, visual.rect.y)
}

impl View {
    /// Opens the presence group of the node `id`, with the transform and the opacity of its presence transition.
    /// `outer` maps the coordinates in which the node paints into those of the view.
    pub(super) fn open_presence(
        &mut self,
        id: NodeId,
        animated: &AnimatedVisual,
        bounds: Rect,
        space: &TransitionSpace,
        outer: Transform,
    ) -> (OpenedPresence, OpenGroup) {
        let visual = &animated.visual;
        let (map, surfaces) = space.presence(visual, animated.transition.bounds, bounds);
        let local = map.transform(position(visual), &visual.presence);
        let mut transform = Slot::Value(local.affine());
        let mut opacity = Slot::Value(visual.presence.alpha());
        if let Some((playing, changing)) = &animated.motion {
            let end = playing.end();
            if changing.moves {
                let presence = playing.projected((position(end), end.presence), |vector| {
                    VisualVectors::of(vector).presenting()
                });
                let motion = TransformMotion::Presence { presence, map };
                transform = Slot::Motion(Box::new(motion));
            }
            if changing.fades {
                let alpha = playing.projected(end.presence.alpha(), |vector| {
                    *PresenceVectors::of(VisualVectors::of(vector).presence).opacity
                });
                opacity = Slot::Motion(Box::new(alpha));
            }
        }
        let open = self.open_group(GroupId::Presence(id), transform, opacity, outer);
        (OpenedPresence { local, surfaces }, open)
    }

    /// Paints the departure of the leaving node `id`, at the position the node had inside the
    /// parent content that starts at `origin`.
    pub(super) fn paint_departure(
        &mut self,
        id: NodeId,
        origin: Point,
        transform: Transform,
        inherited: Inherited,
        text: &mut impl TextMeasurer,
    ) {
        let inherited = Inherited {
            source: Source::Leaving,
            ..inherited
        };
        if let Some((mut apart, position)) = Apart::new(self, id, origin) {
            (apart.view()).paint(0, position, origin, transform, inherited, text);
        }
    }
}

/// The view with the tree of a leaving node in place of the composed tree. Dropping this value puts the composed tree
/// back into the view, discards the hits and the other lists recorded while the departure painted, and returns the tree
/// of the leaving node to its departure.
struct Apart<'a> {
    view: &'a mut View,
    id: NodeId,
    /// The composed tree while the departure paints in its place.
    swapped: LaidOutTree,
    outer: Collected,
}

impl<'a> Apart<'a> {
    fn new(view: &'a mut View, id: NodeId, origin: Point) -> Option<(Self, Point)> {
        let departure = view.transitions.departure(id)?;
        let position = departure.position(origin);
        let mut swapped = departure.take_tree();
        swapped.exchange(view);
        let outer = view.isolate();
        let apart = Self {
            view,
            id,
            swapped,
            outer,
        };
        Some((apart, position))
    }

    fn view(&mut self) -> &mut View {
        self.view
    }
}

impl Drop for Apart<'_> {
    fn drop(&mut self) {
        let view = &mut *self.view;
        self.swapped.exchange(view);
        self.outer.exchange(view);
        if let Some(departure) = view.transitions.departure(self.id) {
            departure.restore_tree(std::mem::take(&mut self.swapped));
        }
    }
}
