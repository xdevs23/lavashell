// SPDX-License-Identifier: GPL-3.0-only

use super::skia_handoff::{Glyphs, SkiaContext, gpu_render_target};
use crate::Error;
use skia_safe::gpu::{self, DirectContext};
use skia_safe::{
    Canvas, Data, Font, IRect, ISize, ImageInfo, Paint, RoundOut, Surface, colors, images, surfaces,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Coverage {
    Exact,
    Corrupt,
    Untested,
}

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

fn coverage(context: &mut DirectContext) -> Result<Coverage, Error> {
    let values = (u8::MIN..=u8::MAX).collect::<Vec<_>>();
    let ramp_size = ISize::new(i32::from(u8::MAX) + 1, 1);
    let ramp_info = ImageInfo::new_a8(ramp_size);
    let ramp = images::raster_from_data(&ramp_info, Data::new_copy(&values), values.len())
        .ok_or(Error::CoverageSurface)?;
    let paint = Paint::new(colors::WHITE, None);
    let ramp_coverage = compare(context, ramp_size, |canvas| {
        let _canvas_returned_for_chaining = canvas.draw_image(&ramp, (0, 0), Some(&paint));
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
        let _canvas_returned_for_chaining = canvas.draw_str(&text, origin, &font, &paint);
    })
}

fn compare(
    context: &mut DirectContext,
    size: ISize,
    draw: impl Fn(&Canvas),
) -> Result<Coverage, Error> {
    let info = ImageInfo::new_n32_premul(size, None);
    let mut device =
        gpu_render_target(context, &info, gpu::Budgeted::No).ok_or(Error::CoverageSurface)?;
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
