// SPDX-License-Identifier: GPL-3.0-only
use crate::scene::Transform;
use crate::scene::tree::GroupValues;
use crate::view::View;
use crate::view::output::{Canvas, GroupId, Painted};
use std::sync::Arc;

pub(in crate::view) struct Detached {
    resting_parent: Transform,
    items: Arc<[Painted]>,
}

impl Detached {
    pub fn moved_by(&self, moved: Transform) -> Self {
        Self {
            resting_parent: moved.then(self.resting_parent),
            items: self.items.clone(),
        }
    }

    pub fn placed(self, id: GroupId, moved: Transform) -> Painted {
        let group = GroupValues {
            transform: moved.then(self.resting_parent).affine(),
            ..GroupValues::default()
        };
        Painted::Group {
            id,
            group: group.into(),
            items: self.items,
        }
    }
}

pub(super) struct OpenDetached {
    resting_parent: Transform,
    around: Canvas,
}

impl View {
    pub(super) fn open_detached(&mut self, parent: Transform) -> OpenDetached {
        OpenDetached {
            resting_parent: self.routing.moving.at_rest(parent),
            around: self.painting.open_items(),
        }
    }

    pub(super) fn close_detached(&mut self, open: OpenDetached) -> Detached {
        Detached {
            resting_parent: open.resting_parent,
            items: self.painting.close_items(open.around).into(),
        }
    }
}
