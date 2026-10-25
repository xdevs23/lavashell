// SPDX-License-Identifier: GPL-3.0-only
use super::catalog::ALBUMS;
use crate::theme::{body, title};
use lavashell::prelude::*;

/// The colors of the art of each album as `0xRRGGBB` values: the start and the end of the background gradient,
/// then the accent color.
const PALETTES: [[u32; 3]; 4] = [
    [0x0031_5b66, 0x001a_3547, 0x00e1_b59a],
    [0x0036_564c, 0x0017_2e2c, 0x00d4_daa5],
    [0x0079_4336, 0x0034_2538, 0x00eb_c39b],
    [0x0078_7eaf, 0x0031_3e69, 0x00e9_cfc6],
];

pub(super) fn album_art(ui: &mut Ui, index: usize, size: Size) {
    let modifier = if size.width > 0.dp() {
        Modifier::new().width(size.width)
    } else {
        Modifier::new().fill_max_width()
    }
    .height(size.height)
    .clip(6.dp());

    ui.canvas(size, modifier, move |d| {
        let Size { width, height } = d.size();
        let colors = PALETTES[index % PALETTES.len()];
        d.fill(
            Rect::new(0.dp(), 0.dp(), width, height),
            Gradient::new(
                GradientKind::Linear {
                    from: Point::default(),
                    to: Point::new(width, height),
                },
                [
                    ColorStop::new(0.0, Color::rgb(colors[0])),
                    ColorStop::new(1.0, Color::rgb(colors[1])),
                ],
            ),
        );
        match index {
            0 => waves(d, colors),
            1 => panes(d, colors),
            2 => bars(d, colors),
            _ => orbs(d, colors),
        }
        if width > 100.dp() && height > 120.dp() {
            cover_text(d, index);
        }
    });
}

fn waves(d: &mut Drawing<'_>, colors: [u32; 3]) {
    const SEGMENTS: u8 = 44;
    let Size { width, height } = d.size();
    d.circle(
        Point::new(width * 0.65, height * 0.34),
        height * 0.24,
        Color::rgb(colors[2]),
    );
    for wave in 0..35_u8 {
        let index = f64::from(wave);
        let y = height * (0.54 + index * 0.017);
        let points: Vec<_> = (0..=SEGMENTS)
            .map(|segment| {
                let x = f64::from(segment) * width / f64::from(SEGMENTS);
                let crest = (x / width * std::f64::consts::TAU * 1.8 + index * 0.24).sin();
                Point::new(x, y + crest * height * (0.008 + index * 0.0004))
            })
            .collect();
        d.path(
            &points,
            false,
            Color::rgba(0x009b_bebd, (35 - wave) * 4 + 20),
            Some(0.8.dp()),
        );
    }
}

fn panes(d: &mut Drawing<'_>, colors: [u32; 3]) {
    let Size { width, height } = d.size();
    for pane in 0..6_u8 {
        let x = width * (0.13 + f64::from(pane) * 0.14);
        d.path(
            &[
                Point {
                    x,
                    y: height * 0.24,
                },
                Point::new(x + width * 0.12, height * 0.14),
                Point::new(x + width * 0.12, height * 0.76),
                Point {
                    x,
                    y: height * 0.87,
                },
            ],
            true,
            Color::rgba(colors[2], 65 + pane * 25),
            Some(1.dp()),
        );
    }
}

fn bars(d: &mut Drawing<'_>, colors: [u32; 3]) {
    let Size { width, height } = d.size();
    for bar in 0..9_u8 {
        let x = width * (0.1 + f64::from(bar) * 0.1);
        d.fill(
            Shape::rounded(
                Rect::new(x, height * 0.18, width * 0.043, height * 0.67),
                width * 0.02,
            ),
            Color::rgba(colors[2], 55 + bar * 20),
        );
    }
    let center = Point::new(width * 0.51, height * 0.51);
    d.circle(center, height * 0.27, Color::rgb(colors[1]));
    d.circle(center, height * 0.14, Color::rgb(colors[0]));
}

fn orbs(d: &mut Drawing<'_>, colors: [u32; 3]) {
    let Size { width, height } = d.size();
    for orb in 0..7_u8 {
        let index = f64::from(orb);
        d.circle(
            Point::new(
                width * (0.2 + index * 0.1),
                height * (0.32 + (index * 0.8).sin() * 0.18),
            ),
            height * (0.1 + index * 0.018),
            Color::rgba(colors[2], 18 + orb * 13),
        );
    }
}

fn cover_text(d: &mut Drawing<'_>, index: usize) {
    let Size { width, height } = d.size();
    let aspect = width / height.max(1.dp());
    let height_in_dp = height.get();
    d.text(
        ALBUMS[index].title.to_uppercase(),
        Rect::new(width * 0.08, height * 0.075, width * 0.84, height * 0.11),
        title(Sp::new(height_in_dp * 0.052).min(13.sp()) * aspect.min(1.2))
            .color(Color::rgb(0x00f6_f0df)),
    );
    d.text(
        ALBUMS[index].artist.to_uppercase(),
        Rect::new(width * 0.08, height * 0.86, width * 0.84, height * 0.07),
        body()
            .size(Sp::new(height_in_dp * 0.037).min(9.sp()))
            .color(Color::rgb(0x00f6_f0df))
            .letter_spacing(0.24.em()),
    );
}
