// SPDX-License-Identifier: GPL-3.0-only
use crate::UiExt;
use lavashell_core::prelude::*;
use lavashell_core::ui::Bounds;

pub(super) fn compose(
    ui: &mut Ui,
    at: impl Into<Deferred<Point, Bounds>>,
    modifier: Modifier,
    content: impl FnOnce(&mut Ui),
) {
    let (popover, view) = (NodeRef::new(), ui.viewport().size);
    let size: Deferred<Size, Bounds> = popover.size().into();
    let at = (at.into().zip(&size)).map(move |(at, size)| inside(at, size, view));
    let placed = Modifier::new().node_ref(&popover).position(at);
    ui.stack(placed.then(modifier), content);
}

fn inside(at: Point, size: Size, view: Size) -> Point {
    let fit = |at: Dp, length: Dp, view: Dp| at.min(view - length).max(Dp::ZERO);
    Point::new(
        fit(at.x, size.width, view.width),
        fit(at.y, size.height, view.height),
    )
}
