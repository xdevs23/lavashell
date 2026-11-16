// SPDX-License-Identifier: GPL-3.0-only
use super::{Hit, View, accessibility, scroll_followers};
use crate::ui::NodeId;
use crate::view::moving::Moving;
use std::collections::BTreeMap;

#[derive(Default)]
pub(super) struct Collected {
    pub hits: Vec<Hit>,
    pub moving: Moving,
    pub geometry: BTreeMap<NodeId, accessibility::SemanticGeometry>,
    pub followers: Vec<scroll_followers::ScrollFollower>,
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
}

impl Collected {
    pub(super) fn exchange(&mut self, view: &mut View) {
        std::mem::swap(&mut self.hits, &mut view.routing.hits);
        std::mem::swap(&mut self.moving, &mut view.routing.moving);
        std::mem::swap(&mut self.geometry, &mut view.accessibility.geometry);
        view.followers.exchange(&mut self.followers);
    }
}
