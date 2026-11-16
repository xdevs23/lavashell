// SPDX-License-Identifier: GPL-3.0-only
use super::Source;
use crate::scene::Transform;
use crate::scene::tree::GroupValues;
use crate::ui::NodeId;
use crate::view::View;
use crate::view::output::{Canvas, GroupId, Painted};
use std::sync::Arc;

pub(in crate::view) struct Above {
    id: NodeId,
    resting_parent: Transform,
    items: Arc<[Painted]>,
}

impl Above {
    pub fn moved_by(&self, moved: Transform) -> Self {
        Self {
            id: self.id,
            resting_parent: moved.then(self.resting_parent),
            items: self.items.clone(),
        }
    }
}

pub(super) struct OpenAbove {
    id: NodeId,
    resting_parent: Transform,
    around: Canvas,
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
            resting_parent: self.routing.moving.at_rest(parent),
            around: self.painting.open_items(),
            earlier: std::mem::take(&mut self.painting.above),
        })
    }

    pub(super) fn close_above(&mut self, open: OpenAbove) {
        let items = self.painting.close_items(open.around).into();
        let inside = std::mem::replace(&mut self.painting.above, open.earlier);
        self.painting.above.push(Above {
            id: open.id,
            resting_parent: open.resting_parent,
            items,
        });
        self.painting.above.extend(inside);
    }

    pub(in crate::view) fn paint_above(&mut self) {
        let canvas = &mut self.painting.canvas;
        for above in self.painting.above.drain(..) {
            let group = GroupValues {
                transform: above.resting_parent.affine(),
                ..GroupValues::default()
            };
            canvas.nest(Painted::Group {
                id: GroupId::Above(above.id),
                group: group.into(),
                items: above.items,
            });
        }
    }
}
