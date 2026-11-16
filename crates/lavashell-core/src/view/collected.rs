// SPDX-License-Identifier: GPL-3.0-only
use super::paint::{Above, Detached};
use super::{Hit, View, accessibility, scroll_followers};
use crate::scene::Transform;
use crate::ui::NodeId;
use crate::view::moving::Moving;
use std::collections::BTreeMap;

#[derive(Default)]
pub(super) struct Collected {
    pub hits: Vec<Hit>,
    pub moving: Moving,
    pub geometry: BTreeMap<NodeId, accessibility::SemanticGeometry>,
    pub followers: Vec<scroll_followers::ScrollFollower>,
    pub above: Vec<Above>,
    pub flown: BTreeMap<NodeId, Detached>,
}

impl View {
    pub(super) fn isolate(&mut self) -> Collected {
        let inside = Collected {
            moving: self.routing.moving.inside(),
            ..Collected::default()
        };
        self.restore(inside)
    }

    pub(super) fn restore(&mut self, mut outer: Collected) -> Collected {
        outer.exchange(self);
        outer
    }

    pub(super) fn keep_painted(&mut self, inside: Collected) {
        let Collected {
            hits: _hits_end_while_the_node_is_in_flight,
            moving,
            geometry: _geometry_ends_while_the_node_is_in_flight,
            followers,
            above,
            flown,
        } = inside;
        (self.routing.moving).place(&moving.into_groups(), Transform::IDENTITY);
        self.followers.extend(followers);
        self.painting.above.extend(above);
        self.painting.flown.extend(flown);
    }
}

impl Collected {
    pub(super) fn exchange(&mut self, view: &mut View) {
        std::mem::swap(&mut self.hits, &mut view.routing.hits);
        std::mem::swap(&mut self.moving, &mut view.routing.moving);
        std::mem::swap(&mut self.geometry, &mut view.accessibility.geometry);
        view.followers.exchange(&mut self.followers);
        std::mem::swap(&mut self.above, &mut view.painting.above);
        std::mem::swap(&mut self.flown, &mut view.painting.flown);
    }
}
