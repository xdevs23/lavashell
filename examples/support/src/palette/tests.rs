// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use lavashell::core::color::Gamut;

const MINIMUM_TEXT_CONTRAST: f64 = 4.5;

fn luminance(color: Color) -> f64 {
    let [red, green, blue] = color.to_linear(Gamut::Srgb);
    0.2126 * red + 0.7152 * green + 0.0722 * blue
}

fn contrast(first: Color, second: Color) -> f64 {
    let [lighter, darker] = [luminance(first), luminance(second)].map(|value| value + 0.05);
    if lighter < darker {
        darker / lighter
    } else {
        lighter / darker
    }
}

#[test]
fn text_stays_readable_on_the_surfaces_it_is_drawn_on() {
    for scheme in [ColorScheme::Dark, ColorScheme::Light] {
        let palette = TealPalette::of(scheme);
        let surfaces = [
            palette.background,
            palette.sidebar,
            palette.card,
            palette.active,
        ];
        for text in [palette.text, palette.accent] {
            for surface in surfaces {
                assert!(
                    contrast(text, surface) >= MINIMUM_TEXT_CONTRAST,
                    "{scheme:?}"
                );
            }
        }
        for surface in &surfaces[..3] {
            assert!(
                contrast(palette.muted, *surface) >= MINIMUM_TEXT_CONTRAST,
                "{scheme:?}"
            );
        }
        assert!(
            contrast(palette.on_accent, palette.accent) >= MINIMUM_TEXT_CONTRAST,
            "{scheme:?}"
        );
    }
}
