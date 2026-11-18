// SPDX-License-Identifier: GPL-3.0-only
use super::key::{ParagraphRaster, TileContent};
use super::tiles::TileSpec;
use super::*;
use crate::test_error::TestError;
use crate::test_surfaces::{ChannelRounding, alpha_pixels, float_pixel, unpremultiplied_pixel};
use lavashell_core::scene::IconStyle;
use lavashell_core::surface::SurfaceId;
use lavashell_core::units::DpExt;
use skia_safe::{Color, Paint, Rect, Vector};

impl RasterCache {
    fn bytes(&self) -> usize {
        self.tiles.values().map(|tile| tile.bytes).sum()
    }
}

pub(super) fn spec(key: u64, bytes: usize, size: (i32, i32), destination: Rect) -> TileSpec {
    TileSpec {
        retained_as: Some(TileContent::paragraph(
            key,
            ParagraphRaster::Native,
            1.0,
            Vector::default(),
        )),
        bytes,
        size: size.into(),
        destination,
    }
}

#[test]
fn a_tile_that_no_recent_frame_drew_is_rasterized_again() -> Result<(), TestError> {
    #[derive(Debug, PartialEq)]
    enum Tile {
        Rasterized,
        Kept,
    }
    let mut cache = RasterCache::new(RasterMode::Tiles, Failures::default());
    let mut surface =
        skia_safe::surfaces::raster_n32_premul((1, 1)).ok_or(TestError::RasterSurface)?;
    let rect = Rect::from_xywh(0.0, 0.0, 1.0, 1.0);
    let mut draw = |cache: &mut RasterCache, key: u64| -> Result<Tile, TestError> {
        let mut tile = Tile::Kept;
        cache.draw_tile(surface.canvas(), &spec(key, 4, (1, 1), rect), |_| {
            tile = Tile::Rasterized;
        })?;
        Ok(tile)
    };
    cache.begin_frame(SurfaceId::numbered(1));
    assert_eq!(draw(&mut cache, 0)?, Tile::Rasterized);
    assert_eq!(draw(&mut cache, 1)?, Tile::Rasterized);
    cache.begin_frame(SurfaceId::numbered(1));
    assert_eq!(
        draw(&mut cache, 0)?,
        Tile::Kept,
        "a tile of the previous frame is kept"
    );
    cache.begin_frame(SurfaceId::numbered(1));
    assert_eq!(draw(&mut cache, 0)?, Tile::Kept);
    assert_eq!(
        draw(&mut cache, 1)?,
        Tile::Rasterized,
        "a tile without a frame that drew it is dropped"
    );
    assert_eq!(cache.bytes(), 2 * 4);
    Ok(())
}

#[test]
fn gradient_path_cache_distinguishes_shaders_and_reuses_equal_gradients() -> Result<(), TestError> {
    use lavashell_core::prelude::{
        Density, DrawContext, Drawing, Gradient, Point, Rect as UiRect, Size, Viewport,
    };
    let mut painter =
        crate::Painter::new(&crate::Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)?
            .with_raster_mode(crate::raster::RasterMode::Tiles);
    let mut surface =
        skia_safe::surfaces::raster_n32_premul((48, 48)).ok_or(TestError::RasterSurface)?;
    for rgb in [0x00ff_0000, 0x0000_00ff, 0x00ff_0000, 0x0000_00ff] {
        let color = lavashell_core::prelude::Color::rgb(rgb);
        let gradient = Gradient::linear(
            Point::new(0.dp(), 0.dp()),
            Point::new(0.dp(), 32.dp()),
            &[color, color.with_alpha(0.0)],
        )?;
        let mut commands = Vec::new();
        let context = DrawContext::new(Viewport::new(Size::new(32.dp(), 32.dp()), Density::ONE));
        Drawing::new(
            &mut commands,
            UiRect::new(0.dp(), 0.dp(), 32.dp(), 32.dp()),
            &context,
        )
        .path(
            &[
                Point::new(0.dp(), 0.dp()),
                Point::new(32.dp(), 0.dp()),
                Point::new(32.dp(), 32.dp()),
                Point::new(0.dp(), 32.dp()),
            ],
            true,
            gradient,
            None,
        );
        let _canvas_returned_for_chaining = surface.canvas().clear(Color::TRANSPARENT);
        painter.paint_as_leaf(surface.canvas(), &commands, Density::ONE)?;
        let pixel = unpremultiplied_pixel(&mut surface, 16, 4);
        let dominant = if rgb == 0x00ff_0000 { 0 } else { 2 };
        assert!(
            pixel[dominant] == 255 && pixel[2 - dominant] == 0,
            "{pixel:?}"
        );
    }
    assert_eq!(painter.resources.tiles.borrow().images().count(), 2);
    Ok(())
}

#[test]
fn scrolling_a_small_path_through_a_clip_reuses_its_complete_tile() -> Result<(), TestError> {
    let mut cache = RasterCache::new(RasterMode::Tiles, Failures::default());
    let mut surface =
        skia_safe::surfaces::raster_n32_premul((40, 40)).ok_or(TestError::RasterSurface)?;
    let mut builder = skia_safe::PathBuilder::new();
    let _builder_returned_for_chaining =
        builder.add_rect(Rect::from_xywh(0.0, 0.0, 10.0, 10.0), None, None);
    let path = builder.detach();
    let paint = Paint::new(skia_safe::Color4f::from(Color::WHITE), None);
    for top in [-10.0, -5.0, 0.0, 5.0] {
        let canvas = surface.canvas();
        let _canvas_returned_for_chaining = canvas.clear(Color::TRANSPARENT);
        let saved = canvas.save();
        let _canvas_returned_for_chaining =
            canvas.clip_rect(Rect::from_xywh(10.0, 10.0, 20.0, 20.0), None, false);
        let _canvas_returned_for_chaining = canvas.translate((10.0, top));
        cache.draw(canvas, &path, &paint)?;
        let _canvas_returned_for_chaining = canvas.restore_to_count(saved);
    }
    assert_eq!(
        cache.images().count(),
        1,
        "clip motion must not create new uploads"
    );
    let pixels = crate::test_surfaces::pixels(&mut surface);
    let pixel = |x, y| pixels.at(x, y);
    assert_eq!(pixel(15, 12), [255, 255, 255, 255]);
    assert_eq!(pixel(15, 8), [0, 0, 0, 0], "the parent clip still applies");
    Ok(())
}

#[test]
fn an_icon_moved_by_whole_pixels_reuses_the_tiles_of_its_paths() -> Result<(), TestError> {
    use lavashell_core::prelude::{
        Density, Dp, DrawContext, Drawing, Icon, IconPath, Point, Rect as UiRect, Segment, Size,
        Viewport,
    };
    const fn at(x: f64, y: f64) -> Point {
        Point::new(Dp::new(x), Dp::new(y))
    }
    static CROSS: Icon = Icon {
        grid: Size::new(Dp::new(10.0), Dp::new(10.0)),
        paths: &[
            IconPath {
                start: at(1.0, 1.0),
                segments: &[Segment::Line(at(9.0, 9.0))],
                closed: false,
                stroke: Some(1.0),
            },
            IconPath {
                start: at(9.0, 1.0),
                segments: &[Segment::Line(at(1.0, 9.0))],
                closed: false,
                stroke: Some(1.0),
            },
        ],
    };
    let mut painter =
        crate::Painter::new(&crate::Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)?
            .with_raster_mode(crate::raster::RasterMode::Tiles);
    let mut surface =
        skia_safe::surfaces::raster_n32_premul((64, 64)).ok_or(TestError::RasterSurface)?;
    let context = DrawContext::new(Viewport::new(Size::new(64.dp(), 64.dp()), Density::ONE));
    for corner in [at(0.0, 0.0), at(3.0, 17.0), at(40.0, 5.0), at(21.0, 42.0)] {
        let mut commands = Vec::new();
        Drawing::new(
            &mut commands,
            UiRect::new(0.dp(), 0.dp(), 64.dp(), 64.dp()),
            &context,
        )
        .icon(
            &CROSS,
            UiRect::new(corner.x, corner.y, 20.dp(), 20.dp()),
            lavashell_core::prelude::Color::WHITE,
            IconStyle::Outlined,
        );
        painter.paint_as_leaf(surface.canvas(), &commands, Density::ONE)?;
        assert_eq!(
            painter.resources.tiles.borrow().images().count(),
            CROSS.paths.len(),
            "{corner:?}"
        );
    }
    Ok(())
}

#[test]
fn floating_point_tiles_keep_hdr_headroom() -> Result<(), TestError> {
    let mut cache = RasterCache::new(RasterMode::Tiles, Failures::default());
    let space = crate::color::working_color_space()?;
    let info = crate::color::working_image_info((8, 8))?;
    let mut surface =
        skia_safe::surfaces::raster(&info, None, None).ok_or(TestError::RasterSurface)?;
    cache.draw_tile(
        surface.canvas(),
        &spec(7, 8 * 8 * 8, (8, 8), Rect::from_xywh(0.0, 0.0, 8.0, 8.0)),
        |tile| {
            let mut paint = Paint::default();
            let _paint_returned_for_chaining =
                paint.set_color4f(skia_safe::Color4f::new(2.0, 0.125, 0.5, 1.0), &space);
            let _canvas_returned_for_chaining = tile.draw_paint(&paint);
        },
    )?;
    let [red, ..] = float_pixel(
        &mut surface,
        (4, 4),
        skia_safe::AlphaType::Premul,
        Some(space),
    );
    let red_channel = f64::from(red);
    assert!(
        (red_channel - 2.0).abs() <= ChannelRounding::of(&surface)?.after(2, 2.0),
        "F16 headroom lost: {red_channel}"
    );
    assert_eq!(cache.bytes(), 8 * 8 * 8);
    Ok(())
}

#[test]
fn a_reused_tile_follows_its_new_position_and_parent_opacity() -> Result<(), TestError> {
    let mut cache = RasterCache::new(RasterMode::Tiles, Failures::default());
    let mut surface =
        skia_safe::surfaces::raster_n32_premul((40, 16)).ok_or(TestError::RasterSurface)?;
    let _canvas_returned_for_chaining = surface.canvas().clear(Color::TRANSPARENT);
    cache.draw_tile(
        surface.canvas(),
        &spec(7, 400, (10, 10), Rect::from_xywh(0.0, 0.0, 10.0, 10.0)),
        |tile| {
            let _canvas_returned_for_chaining = tile.clear(Color::WHITE);
        },
    )?;
    let saved = surface.canvas().save_layer_alpha_f(None, 0.5);
    let mut rasterized = false;
    cache.draw_tile(
        surface.canvas(),
        &spec(7, 400, (10, 10), Rect::from_xywh(20.0, 0.0, 10.0, 10.0)),
        |_| {
            rasterized = true;
        },
    )?;
    assert!(!rasterized, "a cache hit is not rasterized again");
    let _canvas_returned_for_chaining = surface.canvas().restore_to_count(saved);
    let pixels = crate::test_surfaces::pixels(&mut surface);
    let at = |x| pixels.at(x, 5);
    assert_eq!(at(5), [255, 255, 255, 255]);
    assert_eq!(at(25), [128, 128, 128, 128]);
    assert_eq!(at(35), [0, 0, 0, 0]);
    Ok(())
}

#[test]
fn a_large_path_draws_through_kept_tiles_and_only_visible_ones_draw() -> Result<(), TestError> {
    use super::path::Placement;
    let mut cache = RasterCache::new(RasterMode::Tiles, Failures::default());
    let mut surface =
        skia_safe::surfaces::raster_n32_premul((3000, 3000)).ok_or(TestError::RasterSurface)?;
    cache.begin_frame(SurfaceId::numbered(1));
    let canvas = surface.canvas();
    let path = skia_safe::Path::rect(Rect::from_xywh(0.0, 0.0, 3000.0, 3000.0), None);
    let paint = Paint::new(skia_safe::Color4f::from(Color::WHITE), None);
    let Placement::Tiles(tiles) = cache.place(canvas, &path, &paint)? else {
        return Err(TestError::Unexpected("a placement through tiles"));
    };
    assert!(tiles.iter().count() > 1);
    cache.draw(canvas, &path, &paint)?;
    assert_eq!(cache.images().count(), tiles.iter().count());
    let images = |cache: &RasterCache| {
        let mut ids = cache
            .images()
            .map(skia_safe::Image::unique_id)
            .collect::<Vec<_>>();
        ids.sort_unstable();
        ids
    };
    let first = images(&cache);
    let other = skia_safe::Path::circle((1500.0, 1500.0), 1400.0, None);
    cache.begin_frame(SurfaceId::numbered(2));
    cache.draw(surface.canvas(), &other, &paint)?;
    cache.begin_frame(SurfaceId::numbered(1));
    let canvas = surface.canvas();
    cache.draw(canvas, &path, &paint)?;
    assert!(
        first.iter().all(|id| images(&cache).contains(id)),
        "another surface painting between two frames keeps the tiles of the first"
    );
    assert_eq!(unpremultiplied_pixel(&mut surface, 1500, 2990), [255; 4]);

    let canvas = surface.canvas();
    let _canvas_returned_for_chaining =
        canvas.clip_rect(Rect::from_xywh(0.0, 0.0, 600.0, 600.0), None, false);
    let Placement::Tiles(visible) = cache.place(canvas, &path, &paint)? else {
        return Err(TestError::Unexpected("a placement through tiles"));
    };
    let visible = visible.iter().count();
    assert!(visible <= 4, "{visible} tiles for a 600 pixel clip");
    Ok(())
}

fn coverage_and_truth(
    path: &skia_safe::Path,
    size: u16,
    draw: impl FnOnce(&skia_safe::Canvas, &Paint) -> Result<(), TestError>,
) -> Result<Vec<(f64, f64)>, TestError> {
    const SAMPLES: u16 = 8;
    const WINDOW: u16 = 100;
    let mut paint = Paint::new(skia_safe::Color4f::from(Color::WHITE), None);
    let _paint_returned_for_chaining = paint.set_anti_alias(true);
    let square = |side: u16| (i32::from(side), i32::from(side));
    let mut surface =
        skia_safe::surfaces::raster_n32_premul(square(size)).ok_or(TestError::RasterSurface)?;
    draw(surface.canvas(), &paint)?;
    let drawn = alpha_pixels(&mut surface);
    let _paint_returned_for_chaining = paint.set_anti_alias(false);
    let mut pairs = Vec::new();
    for top in (0..size).step_by(usize::from(WINDOW)) {
        for left in (0..size).step_by(usize::from(WINDOW)) {
            let fine_side = WINDOW * SAMPLES;
            let mut fine = skia_safe::surfaces::raster_n32_premul(square(fine_side))
                .ok_or(TestError::RasterSurface)?;
            let _canvas_returned_for_chaining = fine
                .canvas()
                .scale((f32::from(SAMPLES), f32::from(SAMPLES)));
            let _canvas_returned_for_chaining =
                fine.canvas().translate((-f32::from(left), -f32::from(top)));
            let _canvas_returned_for_chaining = fine.canvas().draw_path(path, &paint);
            let samples = alpha_pixels(&mut fine);
            for (y, x) in (0..WINDOW).flat_map(|y| (0..WINDOW).map(move |x| (y, x))) {
                let inside = (0..SAMPLES * SAMPLES)
                    .map(|s| samples.at(x * SAMPLES + s % SAMPLES, y * SAMPLES + s / SAMPLES))
                    .map(f64::from)
                    .sum::<f64>()
                    / f64::from(SAMPLES * SAMPLES);
                if inside > 0.0 && inside < 255.0 {
                    pairs.push((f64::from(drawn.at(left + x, top + y)), inside));
                }
            }
        }
    }
    Ok(pairs)
}

fn errors(pairs: &[(f64, f64)]) -> Result<(f64, f64), TestError> {
    let distances = pairs.iter().map(|(drawn, truth)| (drawn - truth).abs());
    let largest = distances.clone().fold(0.0, f64::max);
    let count = f64::from(u32::try_from(pairs.len())?);
    Ok((largest, distances.sum::<f64>() / count))
}

#[test]
fn a_path_cut_into_tiles_is_as_close_to_true_coverage_as_the_whole_path() -> Result<(), TestError> {
    let star = {
        let mut builder = skia_safe::PathBuilder::new();
        for corner in 0..10_u8 {
            let angle = std::f32::consts::TAU * f32::from(corner) / 10.0 + 0.05;
            let radius = if corner % 2 == 0 { 600.0 } else { 330.0 };
            let point = (0.3 + radius * angle.sin(), 0.7 - radius * angle.cos());
            let _builder_returned_for_chaining = match corner {
                0 => builder.move_to(point),
                _ => builder.line_to(point),
            };
        }
        let _builder_returned_for_chaining = builder.close();
        builder.detach()
    };
    let whole = coverage_and_truth(&star, 800, |canvas, paint| {
        let _canvas_returned_for_chaining = canvas.draw_path(&star, paint);
        Ok(())
    })?;
    let tiled = coverage_and_truth(&star, 800, |canvas, paint| {
        let mut cache = RasterCache::new(RasterMode::Tiles, Failures::default());
        let super::path::Placement::Tiles(tiles) = cache.place(canvas, &star, paint)? else {
            return Err(TestError::Unexpected("a placement through tiles"));
        };
        assert!(tiles.iter().count() > 1, "the star is cut into tiles");
        cache.draw(canvas, &star, paint)?;
        Ok(())
    })?;
    let (whole, tiled) = (errors(&whole)?, errors(&tiled)?);
    assert!(
        tiled.0 <= whole.0 && tiled.1 <= whole.1,
        "tiles {tiled:?} against the whole path {whole:?} (largest, mean)"
    );
    Ok(())
}

#[test]
fn a_tile_without_retained_content_is_neither_reused_nor_retained() -> Result<(), TestError> {
    let mut cache = RasterCache::new(RasterMode::Tiles, Failures::default());
    let mut surface =
        skia_safe::surfaces::raster_n32_premul((8, 8)).ok_or(TestError::RasterSurface)?;
    let rect = Rect::from_xywh(0.0, 0.0, 8.0, 8.0);
    cache.draw_tile(surface.canvas(), &spec(7, 256, (8, 8), rect), |tile| {
        let _canvas_returned_for_chaining = tile.clear(Color::WHITE);
    })?;
    let ids: Vec<_> = cache.images().map(Image::unique_id).collect();
    let transient = TileSpec {
        retained_as: None,
        ..spec(7, 256, (8, 8), rect)
    };
    for (color, pixel) in [
        (Color::RED, [255, 0, 0, 255]),
        (Color::BLUE, [0, 0, 255, 255]),
    ] {
        cache.draw_tile(surface.canvas(), &transient, |tile| {
            let _canvas_returned_for_chaining = tile.clear(color);
        })?;
        assert_eq!(unpremultiplied_pixel(&mut surface, 2, 2), pixel);
        assert_eq!(
            cache.images().map(Image::unique_id).collect::<Vec<_>>(),
            ids
        );
    }
    let mut rasterized = false;
    cache.draw_tile(surface.canvas(), &spec(7, 256, (8, 8), rect), |_| {
        rasterized = true;
    })?;
    assert!(!rasterized, "the retained tile keeps its pixels");
    Ok(())
}
