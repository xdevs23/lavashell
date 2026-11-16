// SPDX-License-Identifier: GPL-3.0-only
use super::Source;
use super::detached::{Detached, OpenDetached};
use crate::scene::Transform;
use crate::ui::NodeId;
use crate::view::View;
use crate::view::output::GroupId;

pub(in crate::view) struct Above {
    id: NodeId,
    painting: Detached,
}

impl Above {
    pub fn moved_by(&self, moved: Transform) -> Self {
        Self {
            id: self.id,
            painting: self.painting.moved_by(moved),
        }
    }
}

pub(super) struct OpenAbove {
    id: NodeId,
    painting: OpenDetached,
    earlier: Vec<Above>,
}

impl View {
    pub(super) fn open_above(
        &mut self,
        index: usize,
        source: Source,
        parent: Transform,
    ) -> Option<OpenAbove> {
        let id = self.nodes[index].id;
        if source != Source::Composition || !self.transitions.paints_above(id) {
            return None;
        }
        Some(OpenAbove {
            id,
            painting: self.open_detached(parent),
            earlier: std::mem::take(&mut self.painting.above),
        })
    }

    pub(super) fn close_above(&mut self, open: OpenAbove) {
        let painting = self.close_detached(open.painting);
        let inside = std::mem::replace(&mut self.painting.above, open.earlier);
        self.painting.above.push(Above {
            id: open.id,
            painting,
        });
        self.painting.above.extend(inside);
    }

    pub(in crate::view) fn paint_above(&mut self) {
        for above in std::mem::take(&mut self.painting.above) {
            let placed = (above.painting).placed(GroupId::Above(above.id), Transform::IDENTITY);
            self.painting.canvas.nest(placed);
        }
    }
}
