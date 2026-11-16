// SPDX-License-Identifier: GPL-3.0-only
use crate::look::PANEL_CORNERS;
use crate::placement::Placement;
use lavashell_core::prelude::*;

#[derive(Clone, Copy, Debug)]
pub(super) struct Area {
    pub size: Size,
    pub panel: Rect,
    pub corners: Corners,
    pub screen: Rect,
}

impl Area {
    pub fn new(placement: Placement, view: Size, panel_height: Dp) -> Self {
        let size = Size::new(view.width, panel_height + placement.below());
        Self {
            size,
            panel: placement.panel(Rect::from_size(size)),
            corners: placement.corners(PANEL_CORNERS),
            screen: Rect::new(Dp::ZERO, size.height - view.height, size.width, view.height),
        }
    }
}
