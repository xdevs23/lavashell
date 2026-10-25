// SPDX-License-Identifier: GPL-3.0-only
//! A check for GPU drivers that corrupt the coverage that Skia samples from alpha masks, such as the glyph atlas of
//! Skia. A mask drawn at whole pixels has the same coverage on a correct device as on the CPU. A difference between
//! the two drawings therefore shows the defect.

use super::skia_handoff::{Glyphs, SkiaContext};
use crate::Error;
use skia_safe::gpu::{self, DirectContext, SurfaceOrigin};
use skia_safe::{
    Canvas, Data, Font, IRect, ISize, ImageInfo, Paint, RoundOut, Surface, colors, images, surfaces,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Coverage {
    Exact,
    Corrupt,
    /// The default font has no glyphs, so the glyph atlas cannot be tested.
    Untested,
}

/// The Skia context created by `create` for the way of drawing glyphs that the device draws correctly. The context
/// uses the glyph atlas when the device draws alpha masks and atlas glyphs exactly as the CPU does. It draws glyphs as
/// paths when the device corrupts the coverage or when the glyph atlas cannot be tested.
pub(super) fn checked_context(
    create: impl Fn(Glyphs) -> Result<SkiaContext, Error>,
) -> Result<(SkiaContext, Glyphs), Error> {
    let mut context = create(Glyphs::Atlas)?;
    if coverage(&mut context)? == Coverage::Exact {
        return Ok((context, Glyphs::Atlas));
    }
    drop(context);
    Ok((create(Glyphs::Paths)?, Glyphs::Paths))
}

/// Draws an alpha mask with every coverage value from 0 to 255, then every printable ASCII glyph of the default font
/// through the glyph atlas. Each drawing is made on the device and on the CPU, and the pixels are compared.
fn coverage(context: &mut DirectContext) -> Result<Coverage, Error> {
    let values = (u8::MIN..=u8::MAX).collect::<Vec<_>>();
    let ramp_size = ISize::new(i32::from(u8::MAX) + 1, 1);
    let ramp_info = ImageInfo::new_a8(ramp_size);
    let ramp = images::raster_from_data(&ramp_info, Data::new_copy(&values), values.len())
        .ok_or(Error::CoverageSurface)?;
    let paint = Paint::new(colors::WHITE, None);
    let ramp_coverage = compare(context, ramp_size, |canvas| {
        let _chained_canvas = canvas.draw_image(&ramp, (0, 0), Some(&paint));
    })?;
    if ramp_coverage == Coverage::Corrupt {
        return Ok(Coverage::Corrupt);
    }
    let font = Font::default();
    let text = (b'!'..=b'~').map(char::from).collect::<String>();
    let bounds: IRect = font.measure_str(&text, None).1.round_out();
    if bounds.is_empty() {
        return Ok(Coverage::Untested);
    }
    compare(context, bounds.size(), |canvas| {
        let origin = (-bounds.left, -bounds.top);
        let _chained_canvas = canvas.draw_str(&text, origin, &font, &paint);
    })
}

/// Whether `draw` gives the same pixels on the device as on the CPU.
fn compare(
    context: &mut DirectContext,
    size: ISize,
    draw: impl Fn(&Canvas),
) -> Result<Coverage, Error> {
    let info = ImageInfo::new_n32_premul(size, None);
    let mut device = gpu::surfaces::render_target(
        context,
        gpu::Budgeted::No,
        &info,
        None,
        SurfaceOrigin::TopLeft,
        None,
        false,
        false,
    )
    .ok_or(Error::CoverageSurface)?;
    let mut reference = surfaces::raster(&info, None, None).ok_or(Error::CoverageSurface)?;
    draw(device.canvas());
    draw(reference.canvas());
    if pixels(&mut device, &info)? == pixels(&mut reference, &info)? {
        Ok(Coverage::Exact)
    } else {
        Ok(Coverage::Corrupt)
    }
}

fn pixels(surface: &mut Surface, info: &ImageInfo) -> Result<Vec<u8>, Error> {
    let mut pixels = vec![0; info.compute_min_byte_size()];
    if surface.read_pixels(info, &mut pixels, info.min_row_bytes(), (0, 0)) {
        Ok(pixels)
    } else {
        Err(Error::CoverageReadback)
    }
}
