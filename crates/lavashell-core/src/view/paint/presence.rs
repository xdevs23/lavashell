// SPDX-License-Identifier: GPL-3.0-only
use super::group::OpenGroup;
use super::{Inherited, Source, Surfaces};
use crate::geometry::{Point, Rect};
use crate::scene::Transform;
use crate::scene::tree::{PresenceMap, PresenceVectors, Slot, TransformMotion};
use crate::ui::NodeId;
use crate::view::collected::Collected;
use crate::view::laid_out::LaidOutTree;
use crate::view::output::GroupId;
use crate::view::transition::{AnimatedVisual, BoundsMotion, Visual, VisualVectors};
use crate::view::{TextMeasurer, View};

pub(super) struct OpenedPresence {
    pub local: Transform,
    pub surfaces: Surfaces,
}

pub(super) struct TransitionSpace {
    pub origin: Point,
    pub outer: Transform,
}

impl TransitionSpace {
    pub fn in_view(resting: Transform, laid_out: Point, viewport: Point) -> Self {
        Self {
            origin: Point::new(-laid_out.x - viewport.x, -laid_out.y - viewport.y),
            outer: resting.inverse().then(Transform::translation(laid_out)),
        }
    }

    pub fn in_parent(origin: Point) -> Self {
        Self {
            origin,
            outer: Transform::IDENTITY,
        }
    }

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

struct Apart<'a> {
    view: &'a mut View,
    id: NodeId,
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
