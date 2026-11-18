// SPDX-License-Identifier: GPL-3.0-only
use super::model::App;
use lavashell::prelude::*;
use lavashell_example_support::{BODY, DISPLAY};
use std::time::Duration;

pub const INK: Color = Color::oklch(0.95, 0.012, 255.0);
pub const MUTED: Color = Color::oklch(0.78, 0.025, 255.0);
const ON_ACCENT: Color = Color::oklch(0.99, 0.004, 255.0);
const RING: Color = Color::oklch(0.88, 0.11, 205.0);
const DARK_TINT: Color = Color::oklch(0.26, 0.035, 262.0);
const LIGHT_TINT: Color = Color::oklch(0.66, 0.07, 228.0);
pub const ICON_SIZE: Dp = Dp::new(22.0);
pub const BUTTON_PADDING: Dp = Dp::new(13.0);
pub const BUTTON_HEIGHT: Dp = Dp::new(ICON_SIZE.get() + 2.0 * BUTTON_PADDING.get());
pub const BUTTON_RADIUS: Dp = Dp::new(BUTTON_HEIGHT.get() / 2.0);
pub const ISLAND_PADDING: Dp = Dp::new(8.0);
pub const ISLAND_RADIUS: Dp = Dp::new(BUTTON_RADIUS.get() + ISLAND_PADDING.get());
pub const SPACING: Dp = Dp::new(28.0);
pub const SCREEN: Size = Size::new(Dp::new(1280.0), Dp::new(720.0));

#[must_use]
pub fn spring() -> Spring {
    spring!(Duration::from_millis(620), 0.72)
}

#[must_use]
pub fn body() -> TextStyle {
    TextStyle::new(BODY, 15.sp(), INK)
}

#[must_use]
pub fn title() -> TextStyle {
    TextStyle::new(DISPLAY, 24.sp(), INK).weight(FontWeight::BOLD)
}

#[must_use]
pub fn glass(scheme: ColorScheme) -> Material {
    const LENS: Glass = Glass {
        refraction: constant!(NonNegativeDp::try_new(Dp::new(22.0))),
        dispersion: constant!(NonNegativeDp::try_new(Dp::new(1.2))),
        rim: constant!(NonNegative::try_new(0.32)),
    };
    let tint = scheme.select(DARK_TINT, LIGHT_TINT);
    Material::new(tint.with_alpha(0.32))
        .backdrop(constant!(Backdrop::glass(Dp::new(4.0), LENS)))
        .border(constant!(Stroke::new(
            Color::WHITE.with_alpha(0.22),
            Dp::new(1.0)
        )))
        .shadow(constant!(Shadow::new(
            Color::oklch(0.1, 0.02, 265.0).with_alpha(0.5),
            Dp::new(16.0),
            Point::new(Dp::ZERO, Dp::new(8.0)),
        )))
}

#[must_use]
pub fn slot(accent: Color) -> StateStyles {
    styles(move |state| {
        let appearance = common(state);
        let appearance = if state.pointer.active && !state.selected {
            appearance.content(lighter(accent))
        } else {
            appearance
        };
        lit(state.selected, accent, appearance)
    })
}

#[must_use]
pub fn media() -> StateStyles {
    styles(|state| lit(state.selected, App::Music.accent(), common(state)))
}

#[must_use]
pub fn status() -> StateStyles {
    styles(|state| {
        InteractionAppearance::default()
            .scale(if state.pointer.active {
                constant!(ScaleFactor::try_new(0.97))
            } else {
                ScaleFactor::ONE
            })
            .outline(ring(state.focus.visible, ISLAND_RADIUS))
    })
}

fn styles(resolve: impl Fn(InteractionState) -> InteractionAppearance + 'static) -> StateStyles {
    StateStyles::new(resolve).motion(spring())
}

fn common(state: InteractionState) -> InteractionAppearance {
    InteractionAppearance::default()
        .tint(
            Color::WHITE.with_alpha(if state.pointer.active { 0.14 } else { 0.0 }),
            BUTTON_RADIUS,
        )
        .scale(if state.pointer.active {
            constant!(ScaleFactor::try_new(0.92))
        } else {
            ScaleFactor::ONE
        })
        .outline(ring(state.focus.visible, BUTTON_RADIUS))
}

fn ring(visible: bool, radius: Dp) -> BorderStyle {
    if visible {
        BorderStyle {
            color: RING,
            width: constant!(NonNegativeDp::try_new(Dp::new(2.0))),
            radius,
        }
    } else {
        BorderStyle::NONE
    }
}

const GLOW: Shadow = constant!(Shadow::new(
    Color::TRANSPARENT,
    Dp::new(10.0),
    Point::new(Dp::ZERO, Dp::new(4.0)),
));

fn lit(selected: bool, accent: Color, appearance: InteractionAppearance) -> InteractionAppearance {
    if !selected {
        return appearance;
    }
    let kind = GradientKind::Linear {
        from: Point::new(0.dp(), 0.dp()),
        to: Point::new(0.dp(), BUTTON_HEIGHT),
    };
    let stops = [
        ColorStop::new(0.0, Color::WHITE.with_alpha(0.3)),
        ColorStop::new(1.0, Color::WHITE.with_alpha(0.0)),
    ];
    let shine = Gradient::new(kind, stops);
    let material = Material::new(accent)
        .shading(shine)
        .border(constant!(Stroke::new(
            Color::WHITE.with_alpha(0.28),
            Dp::new(1.0)
        )))
        .shadow(Shadow {
            color: accent.with_alpha(0.5),
            ..GLOW
        });
    appearance
        .material(material, BUTTON_RADIUS)
        .content(ON_ACCENT)
}

fn lighter(color: Color) -> Color {
    let [_replaced_lightness, chroma, hue] = color.to_oklch();
    Color::oklch(0.84, chroma * 0.6, hue)
}
