// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::failure::{DrawFailure, FailedDraw};
use crate::field::test_shapes::{Distance, star, union};
use crate::field::{FieldRoom, Fields, Solid, UniformRange};
use crate::test_error::TestError;
use lavashell_core::surface::SurfaceId;
use skia_safe::surfaces;

fn rounded(x: f64, y: f64, width: f64, corners: Corners) -> Shape {
    RoundedRect::new(
        Rect::new(Dp::new(x), Dp::new(y), Dp::new(width), 40.dp()),
        corners,
    )
    .into()
}

#[test]
fn field_textures_match_shape_distance_at_texel_centres() -> Result<(), TestError> {
    let row = union(
        (0..10_u8).map(|i| rounded(4.0 + f64::from(i) * 19.0, 40.0, 16.0, Corners::all(8.dp()))),
        10.dp(),
    )?;
    let nested = union(
        [
            star(12, 30.0)?,
            union(
                [
                    rounded(
                        10.0,
                        70.0,
                        60.0,
                        Corners::new(20.dp(), 0.dp(), 4.dp(), 12.dp()),
                    ),
                    rounded(74.0, 74.0, 40.0, Corners::all(20.dp())),
                ],
                16.dp(),
            )?,
        ],
        20.dp(),
    )?;
    let info = ImageInfo::new((200, 120), ColorType::RGBAF16, AlphaType::Premul, None);
    let mut surface = surfaces::raster(&info, None, None).ok_or(TestError::RasterSurface)?;
    let fields = Fields::new(&FieldRoom::new(UniformRange::VULKAN_REQUIRED))?;
    for shape in [star(40, 30.0)?, row, nested] {
        let field = fields
            .exact(surface.canvas(), &shape, 0.0, TexelPixels::DEVICE_PIXEL)?
            .ok_or(TestError::Missing("field texture"))?;
        let read = ImageInfo::new(field.grid.size, ColorType::RGBAF32, AlphaType::Premul, None);
        let row_bytes = read.min_row_bytes();
        let mut bytes = vec![0u8; read.compute_min_byte_size()];
        assert!(field.image.read_pixels(
            &read,
            &mut bytes,
            row_bytes,
            (0, 0),
            skia_safe::image::CachingHint::Allow,
        ));
        let centers = || std::iter::successors(Some(0.5_f64), |center| Some(center + 1.0));
        let texels = bytes
            .chunks(row_bytes)
            .zip(centers())
            .flat_map(|(line, row)| {
                line.chunks(read.bytes_per_pixel())
                    .zip(centers())
                    .map(move |(texel, column)| (texel, column, row))
            });
        for (texel, column, row) in texels {
            let p = Point::new(
                field.grid.region.x + Dp::new(column * field.grid.texel),
                field.grid.region.y + Dp::new(row * field.grid.texel),
            );
            let expected = shape.distance(p);
            let found = f64::from(f32::from_ne_bytes(texel[..4].try_into()?));
            // Every nesting level rounds the distance to a half float once more.
            assert!(
                (found - expected).abs() <= 0.02 + expected.abs() * 2e-3,
                "{shape:?} at {p:?}: texture {found} and CPU {expected}"
            );
        }
    }
    Ok(())
}

/// The unique ID of the Skia image in `texture`. A texture returned again from storage has the same ID.
fn image_id(texture: Result<Option<FieldTexture>, FailedDraw>) -> Result<u32, TestError> {
    let texture = texture?.ok_or(TestError::Missing("field texture"))?;
    Ok(texture.image.unique_id())
}

#[test]
fn every_part_of_a_frame_reads_one_texture_of_a_shape() -> Result<(), TestError> {
    let mut surface = surfaces::raster_n32_premul((200, 120)).ok_or(TestError::RasterSurface)?;
    let canvas = surface.canvas();
    let mut fields = Fields::new(&FieldRoom::new(UniformRange::VULKAN_REQUIRED))?;
    fields.begin_frame(SurfaceId(0));
    let shape = star(40, 30.0)?;
    let shadow = image_id(fields.exact(canvas, &shape, 24.0, TexelPixels::DEVICE_PIXEL))?;
    assert_eq!(
        image_id(fields.exact(canvas, &shape, 0.0, TexelPixels::DEVICE_PIXEL))?,
        shadow
    );
    let lens = image_id(fields.lens(canvas, &Solid::of(&shape), TexelPixels::DEVICE_PIXEL))?;
    assert_eq!(
        image_id(fields.lens(canvas, &Solid::of(&shape), TexelPixels::DEVICE_PIXEL))?,
        lens
    );
    assert_eq!(
        image_id(fields.exact(canvas, &shape, 4.0, TexelPixels::DEVICE_PIXEL))?,
        shadow
    );

    let wider = image_id(fields.exact(canvas, &shape, 40.0, TexelPixels::DEVICE_PIXEL))?;
    assert_ne!(wider, shadow, "a texture that covers less is rebuilt");
    assert_eq!(
        image_id(fields.exact(canvas, &shape, 24.0, TexelPixels::DEVICE_PIXEL))?,
        wider
    );
    Ok(())
}

#[test]
fn a_field_texture_stays_while_frames_draw_its_shape() -> Result<(), TestError> {
    let mut surface = surfaces::raster_n32_premul((200, 120)).ok_or(TestError::RasterSurface)?;
    let canvas = surface.canvas();
    let mut fields = Fields::new(&FieldRoom::new(UniformRange::VULKAN_REQUIRED))?;
    let shape = star(40, 30.0)?;
    let id = |fields: &mut Fields| {
        fields.begin_frame(SurfaceId(0));
        image_id(fields.exact(canvas, &shape, 0.0, TexelPixels::DEVICE_PIXEL))
    };
    let first = id(&mut fields)?;
    assert_eq!(id(&mut fields)?, first, "kept for the next frame");
    fields.begin_frame(SurfaceId(0));
    assert_ne!(id(&mut fields)?, first, "dropped after a frame without it");
    Ok(())
}

#[test]
fn a_field_texture_that_cannot_be_allocated_fails() -> Result<(), TestError> {
    let mut surface = surfaces::raster_n32_premul((200, 120)).ok_or(TestError::RasterSurface)?;
    let fields = Fields::new(&FieldRoom::new(UniformRange::VULKAN_REQUIRED))?;
    assert_eq!(
        fields
            .exact(
                surface.canvas(),
                &star(40, 30.0)?,
                1.0e7,
                TexelPixels::DEVICE_PIXEL
            )
            .err()
            .map(|failed| failed.failure()),
        Some(DrawFailure::Allocation)
    );
    Ok(())
}

#[test]
fn a_joined_grid_covers_both_regions_or_falls_back_where_too_long() -> Result<(), TestError> {
    let mut surface = surfaces::raster_n32_premul((8, 8)).ok_or(TestError::RasterSurface)?;
    let square = |x: f64| Grid::new(Rect::new(Dp::new(x), 0.dp(), 10.dp(), 10.dp()), 1.0);
    let beyond_any_side = f64::from(i32::MAX) * 2.0;
    let (first, near, far) = (square(0.0)?, square(100.0)?, square(beyond_any_side)?);
    let joined = first.joined(&near, surface.canvas())?;
    assert!(joined.covers(&first) && joined.covers(&near));
    assert!(first.joined(&far, surface.canvas())? == far);
    Ok(())
}
