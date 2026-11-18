// SPDX-License-Identifier: GPL-3.0-only
use super::super::{PixelGeometry, TextAntialiasing, TextDestination, TextRendering};
use crate::test_error::TestError;
use crate::test_font::probe_style_at;
use crate::test_surfaces::TestSurface;
use lavashell_core::prelude::*;
use num_traits::ToPrimitive;
use std::ops::Range;

pub fn new_painter(geometry: PixelGeometry) -> Result<crate::Painter, TestError> {
    let fonts = crate::test_font::test_font()?;
    let mut painter =
        crate::Painter::without_device(&fonts)?.with_raster_mode(crate::raster::RasterMode::Tiles);
    painter.set_text_rendering(
        TextRendering {
            antialiasing: TextAntialiasing::SubpixelApproximate,
            ..Default::default()
        },
        TextDestination {
            opaque: false,
            geometry,
        },
    );
    Ok(painter)
}

pub const SAMPLE_SURFACE: TestSurface = TestSurface::new(900, 340);
const BACKDROPS: [Color; 3] = [
    Color::TRANSPARENT,
    Color::rgba(0x0011_111a, 245),
    Color::rgb(0x0011_111a),
];
const COLUMN_WIDTH: f64 = 300.0;
const TEXT_LEFT: f64 = 12.25;
const TEXT_WIDTH: f64 = 276.0;
const SMALL_TEXT_TOP: f64 = 8.5;
const SMALL_TEXT_ROW_PITCH: f64 = 52.0;
const SMALL_TEXT_LINE_PITCH: f64 = 24.0;
const SMALL_TEXT_HEIGHT: f64 = 24.0;
const LARGE_TEXT_TOP: f64 = 176.5;
const LARGE_TEXT_HEIGHT: f64 = 140.0;

fn column_left(column: u8) -> f64 {
    COLUMN_WIDTH * f64::from(column)
}

fn pixels_covering(start: f64, end: f64) -> Result<Range<u16>, TestError> {
    let whole_pixel = |edge: f64| {
        edge.to_u16()
            .ok_or(TestError::Unexpected("a span of whole pixels inside u16"))
    };
    Ok(whole_pixel(start.floor())?..whole_pixel(end.ceil())?)
}

pub fn transparent_backdrop_columns() -> Result<Range<u16>, TestError> {
    let column = (BACKDROPS.iter())
        .position(|backdrop| *backdrop == Color::TRANSPARENT)
        .ok_or(TestError::Missing("transparent backdrop"))?;
    let left = column_left(u8::try_from(column)?);
    pixels_covering(left, left + COLUMN_WIDTH)
}

pub fn small_text_rows() -> Result<Range<u16>, TestError> {
    pixels_covering(0.0, LARGE_TEXT_TOP)
}

pub fn large_text_rows() -> Result<Range<u16>, TestError> {
    pixels_covering(LARGE_TEXT_TOP, LARGE_TEXT_TOP + LARGE_TEXT_HEIGHT)
}

#[must_use]
pub fn commands() -> Vec<DrawCommand> {
    let mut commands = Vec::new();
    for (background, column) in BACKDROPS.into_iter().zip(0_u8..) {
        let x = column_left(column).dp();
        commands.push(DrawCommand::Fill {
            shape: Rect::new(x, 0.dp(), COLUMN_WIDTH.dp(), SAMPLE_SURFACE.size().height).into(),
            brush: background.into(),
        });
        for (size, row) in [11.0, 13.0, 16.0].into_iter().zip(0_u8..) {
            for (alpha, line) in [1.0, 0.4].into_iter().zip(0_u8..) {
                let mut style = probe_style_at(Sp::new(size));
                style.color = Color::WHITE.with_alpha(alpha);
                commands.push(DrawCommand::Text {
                    text: "Headphones p".into(),
                    rect: Rect::new(
                        x + TEXT_LEFT.dp(),
                        (SMALL_TEXT_TOP
                            + SMALL_TEXT_ROW_PITCH * f64::from(row)
                            + SMALL_TEXT_LINE_PITCH * f64::from(line))
                        .dp(),
                        TEXT_WIDTH.dp(),
                        SMALL_TEXT_HEIGHT.dp(),
                    ),
                    style,
                });
            }
        }
        let mut style = probe_style_at(100.sp());
        style.color = Color::WHITE;
        commands.push(DrawCommand::Text {
            text: "12:34".into(),
            rect: Rect::new(
                x + TEXT_LEFT.dp(),
                LARGE_TEXT_TOP.dp(),
                TEXT_WIDTH.dp(),
                LARGE_TEXT_HEIGHT.dp(),
            ),
            style,
        });
    }
    commands
}
