// SPDX-License-Identifier: GPL-3.0-only
use lavashell::prelude::*;

/// An album cover: a sun setting into the sea, in colors around `hue`.
pub fn cover(d: &mut Drawing<'_>, hue: f64) {
    let side = d.size().width;
    let at = |x: f64, y: f64| Point::new(side * x, side * y);
    let square = Rect::new(Dp::ZERO, Dp::ZERO, side, side);
    let corners = side * 0.22;
    let deep = Color::oklch(0.42, 0.16, hue);
    let kind = GradientKind::Linear {
        from: at(0.0, 0.0),
        to: at(0.0, 1.0),
    };
    let glow = Color::oklch(0.74, 0.15, hue + 60.0);
    let sky = Gradient::new(kind, [ColorStop::new(0.0, deep), ColorStop::new(1.0, glow)]);
    d.fill(Shape::rounded(square, corners), sky);
    d.save();
    d.clip(square, corners);
    d.circle(
        at(0.5, 0.64),
        side * 0.27,
        Color::oklch(0.92, 0.12, hue + 95.0),
    );
    for band in [0.68, 0.78, 0.88] {
        let wave = Rect::new(Dp::ZERO, side * band, side, side * 0.045);
        d.fill(wave, deep.with_alpha(0.85));
    }
    d.restore();
}
