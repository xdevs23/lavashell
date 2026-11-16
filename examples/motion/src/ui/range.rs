// SPDX-License-Identifier: GPL-3.0-only
use crate::theme::PALETTE;
use lavashell::prelude::*;

const THUMB_RADIUS: Dp = Dp::new(7.0);

pub(super) fn slider(
    ui: &mut Ui,
    value: impl Fn() -> f64 + 'static,
    width: Dp,
    modifier: Modifier,
    change: impl FnMut(f64) + 'static,
) {
    let colors = ui.local(&PALETTE);
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
                    colors.pale,
                );
                d.fill(
                    Shape::rounded(Rect::new(0.dp(), 17.dp(), width * value, 4.dp()), 2.dp()),
                    colors.blue,
                );
                d.circle(
                    Point::new(width * value, 19.dp()),
                    THUMB_RADIUS,
                    colors.blue,
                );
            });
        },
    );
}
