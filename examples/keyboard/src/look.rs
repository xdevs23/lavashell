// SPDX-License-Identifier: GPL-3.0-only
use lavashell::prelude::*;
use lavashell_example_support::BODY;

pub struct Palette {
    pub ink: Color,
    pub muted: Color,
    pub accent: Color,
    pub app: Color,
    pub on_app: Color,
    pub navigation: Color,
    pub backdrop: [Color; 2],
    pub window: Color,
    pub window_edge: Color,
    pub field: Color,
    pub clear: Color,
    pub taskbar: Color,
    pub taskbar_edge: Color,
    pub launcher: Color,
    pub launcher_ring: Color,
}

/// The colors of the dark screen. The white edges have a low alpha, because the view blends in linear light, where
/// a translucent white over a dark color looks brighter than with blending in sRGB.
pub const NIGHT: Palette = Palette {
    ink: Color::oklch(0.963, 0.007, 258.0),
    muted: Color::oklch(0.78, 0.012, 258.0),
    accent: Color::oklch(0.825, 0.08, 260.0),
    app: Color::oklch(0.74, 0.15, 40.0),
    on_app: Color::WHITE,
    navigation: Color::oklch(0.627, 0.141, 148.0),
    backdrop: [
        Color::oklch(0.315, 0.032, 258.0),
        Color::oklch(0.372, 0.032, 258.0),
    ],
    window: Color::oklch(0.205, 0.012, 258.0),
    window_edge: Color::WHITE.with_alpha(0.025),
    field: Color::oklch(0.265, 0.012, 258.0),
    clear: Color::oklch(0.34, 0.012, 258.0),
    taskbar: Color::oklch(0.158, 0.007, 258.0).with_alpha(0.82),
    taskbar_edge: Color::WHITE.with_alpha(0.045),
    launcher: Color::oklch(0.3, 0.01, 258.0),
    launcher_ring: Color::oklch(0.595, 0.007, 258.0),
};

pub const DAY: Palette = Palette {
    ink: Color::oklch(0.25, 0.02, 258.0),
    muted: Color::oklch(0.48, 0.02, 258.0),
    accent: Color::oklch(0.47, 0.13, 260.0),
    app: Color::oklch(0.6, 0.17, 40.0),
    on_app: Color::WHITE,
    navigation: Color::oklch(0.56, 0.141, 148.0),
    backdrop: [
        Color::oklch(0.86, 0.02, 258.0),
        Color::oklch(0.8, 0.025, 258.0),
    ],
    window: Color::oklch(0.975, 0.004, 258.0),
    window_edge: Color::oklch(0.27, 0.037, 264.0).with_alpha(0.06),
    field: Color::oklch(0.92, 0.008, 258.0),
    clear: Color::oklch(0.86, 0.01, 258.0),
    taskbar: Color::oklch(0.97, 0.006, 258.0).with_alpha(0.82),
    taskbar_edge: Color::WHITE.with_alpha(0.9),
    launcher: Color::oklch(0.88, 0.01, 258.0),
    launcher_ring: Color::oklch(0.6, 0.01, 258.0),
};

pub fn text(color: Color, size: Sp) -> TextStyle {
    TextStyle::new(BODY, size, color)
}

pub fn interaction(palette: &Palette) -> InteractionStyle {
    InteractionStyle {
        placeholder: Some(palette.muted),
        caret: Some(palette.app),
        selection: Some(palette.app.with_alpha(0.35)),
        ..InteractionStyle::default()
    }
}

pub fn backdrop(palette: &Palette, size: Size) -> Gradient {
    let kind = GradientKind::Linear {
        from: Point::new(size.width * 0.4, Dp::ZERO),
        to: Point::new(size.width * 0.6, size.height),
    };
    let [start, end] = palette.backdrop;
    Gradient::new(kind, [ColorStop::new(0.0, start), ColorStop::new(1.0, end)])
}
