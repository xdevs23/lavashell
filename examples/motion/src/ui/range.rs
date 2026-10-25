// SPDX-License-Identifier: GPL-3.0-only
use crate::theme::{BLUE, PALE};
use lavashell::prelude::*;

const THUMB_RADIUS: Dp = Dp::new(7.0);

/// A slider `width` wide. Page Up and Page Down move the thumb by the diameter of the thumb.
pub(super) fn slider(
    ui: &mut Ui,
    value: impl Fn() -> f64 + 'static,
    width: Dp,
    modifier: Modifier,
    change: impl FnMut(f64) + 'static,
) {
    let page = THUMB_RADIUS * 2.0 / width;
    ui.slider(
        value,
        Some(page),
        Modifier::new().size(width, 38.dp()).then(modifier),
        change,
        move |ui, value| {
            ui.canvas(Size::new(width, 38.dp()), Modifier::new(), move |d| {
                let value = value();
                d.fill(
                    Shape::rounded(Rect::new(0.dp(), 17.dp(), width, 4.dp()), 2.dp()),
                    PALE,
                );
                d.fill(
                    Shape::rounded(Rect::new(0.dp(), 17.dp(), width * value, 4.dp()), 2.dp()),
                    BLUE,
                );
                d.circle(Point::new(width * value, 19.dp()), THUMB_RADIUS, BLUE);
            });
        },
    );
}
