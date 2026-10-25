// SPDX-License-Identifier: GPL-3.0-only
use super::{BTreeMap, Hit, Moving, NodeId, View, accessibility, scroll_followers};

/// The lists recorded while the view paints, apart from the drawing commands in the canvas: the input hits, the
/// moving groups, the accessibility geometry and the scroll followers.
#[derive(Default)]
pub(super) struct Collected {
    pub hits: Vec<Hit>,
    pub moving: Moving,
    pub geometry: BTreeMap<NodeId, accessibility::SemanticGeometry>,
    pub followers: Vec<scroll_followers::ScrollFollower>,
}

impl View {
    /// Makes the view record into empty lists from here on, and returns the lists recorded before. The new list of
    /// moving groups starts from the transform at rest of the moving groups that enclose this point.
    pub(super) fn isolate(&mut self) -> Collected {
        let inside = Collected {
            moving: self.moving.inside(),
            ..Collected::default()
        };
        self.restore(inside)
    }

    /// Puts the lists in `outer`, such as the ones returned by `isolate`, back into the view, and returns the lists
    /// recorded since `isolate`.
    pub(super) fn restore(&mut self, mut outer: Collected) -> Collected {
        outer.exchange(self);
        outer
    }
}

impl Collected {
    /// Swaps these lists with the ones that the view records into.
    pub(super) fn exchange(&mut self, view: &mut View) {
        std::mem::swap(&mut self.hits, &mut view.hits);
        std::mem::swap(&mut self.moving, &mut view.moving);
        std::mem::swap(&mut self.geometry, &mut view.accessibility_geometry);
        view.followers.exchange(&mut self.followers);
    }
}
