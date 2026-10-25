// SPDX-License-Identifier: GPL-3.0-only
use crate::color::channel_step;
use crate::destination::Destination;
use crate::failure::{DrawFailure, FailedDraw};
use crate::field::Band;
use crate::paint::{sklength, skrect, solid};
use crate::resources::PaintResources;
use crate::silhouette::Silhouette;
use lavashell_core::prelude::*;
use skia_safe::canvas::SaveLayerRec;
use skia_safe::image_filters;
use skia_safe::{Canvas, ClipOp, Paint};

/// Opens a layer for an effect, blended with `mode`, blurred by `sigma` and filtered by `filter`. When an argument
/// cannot be converted for Skia, the function fails before it changes the canvas.
pub(crate) fn layer(
    canvas: &Canvas,
    bounds: Rect,
    mode: BlendMode,
    sigma: NonNegativeDp,
    filter: Option<&SourceFilter>,
    expanding_input: bool,
) -> Result<Destination, DrawFailure> {
    let layout_bounds = skrect(bounds)?;
    let mut paint = Paint::default();
    let _chained_paint = paint.set_blend_mode(blend(mode));
    let sigma = sklength(sigma)?;
    if sigma > 0.0 {
        let _chained_paint = paint.set_image_filter(image_filters::blur(
            (sigma, sigma),
            skia_safe::TileMode::Decal,
            None,
            None,
        ));
    }
    if let Some(filter) = filter
        && let Some(filtered) = crate::source_filters::resolve(filter, bounds)?
    {
        let _chained_paint = paint.set_image_filter(match paint.image_filter() {
            Some(inner) => image_filters::compose(filtered, inner),
            None => Some(filtered),
        });
    }
    let save = SaveLayerRec::default().paint(&paint);
    if expanding_input || paint.image_filter().is_some() {
        // A filter on this layer or inside it can draw past the layout bounds of this element. Without bounds, Skia
        // sizes the layer from the current clip, which covers that output. A layer limited to the layout bounds
        // would cut the output off before an enclosing filter reads it.
        let _save_count = canvas.save_layer(&save);
    } else {
        let _save_count = canvas.save_layer(&save.bounds(&layout_bounds));
    }
    Ok(Destination::Layer)
}

pub(crate) fn fill(
    canvas: &Canvas,
    resources: &PaintResources,
    shape: &Shape,
    paint: &Paint,
) -> Result<(), FailedDraw> {
    match Silhouette::new(canvas, resources, shape, 0.0)? {
        Some(silhouette) => silhouette.fill(canvas, paint).map_err(FailedDraw::from),
        None => Ok(()),
    }
}

pub(crate) fn stroke(
    canvas: &Canvas,
    resources: &PaintResources,
    shape: &Shape,
    stroke: Stroke,
) -> Result<(), FailedDraw> {
    if stroke.width == NonNegativeDp::ZERO {
        return Ok(());
    }
    let band = Band::stroke(stroke);
    match Silhouette::new(canvas, resources, shape, band.reach)? {
        Some(silhouette) => silhouette.band(canvas, band, &solid(stroke.color)?),
        None => Ok(()),
    }
}

pub(crate) fn shadow(
    canvas: &Canvas,
    resources: &PaintResources,
    shape: &Shape,
    shadow: Shadow,
) -> Result<(), FailedDraw> {
    let op = if shadow.inset {
        ClipOp::Intersect
    } else {
        ClipOp::Difference
    };
    // A canvas without color channels shows no shadow.
    let Some(step) = channel_step(canvas.image_info().color_type()) else {
        return Ok(());
    };
    let band = Band::shadow(shadow, step);
    let paint = solid(shadow.color)?;
    let Some(silhouette) = Silhouette::new(canvas, resources, shape, band.reach)? else {
        return Ok(());
    };
    let saved = canvas.save();
    let painted = silhouette
        .clip(canvas, op)
        .map_err(FailedDraw::from)
        .and_then(|()| silhouette.band(canvas, band, &paint));
    let _chained_canvas = canvas.restore_to_count(saved);
    painted
}

pub(crate) fn blend(mode: BlendMode) -> skia_safe::BlendMode {
    use skia_safe::BlendMode as S;
    match mode {
        BlendMode::Clear => S::Clear,
        BlendMode::Source => S::Src,
        BlendMode::Destination => S::Dst,
        BlendMode::SourceOver => S::SrcOver,
        BlendMode::DestinationOver => S::DstOver,
        BlendMode::SourceIn => S::SrcIn,
        BlendMode::DestinationIn => S::DstIn,
        BlendMode::SourceOut => S::SrcOut,
        BlendMode::DestinationOut => S::DstOut,
        BlendMode::SourceAtop => S::SrcATop,
        BlendMode::DestinationAtop => S::DstATop,
        BlendMode::Xor => S::Xor,
        BlendMode::Plus => S::Plus,
        BlendMode::Modulate => S::Modulate,
        BlendMode::Screen => S::Screen,
        BlendMode::Overlay => S::Overlay,
        BlendMode::Darken => S::Darken,
        BlendMode::Lighten => S::Lighten,
        BlendMode::ColorDodge => S::ColorDodge,
        BlendMode::ColorBurn => S::ColorBurn,
        BlendMode::HardLight => S::HardLight,
        BlendMode::SoftLight => S::SoftLight,
        BlendMode::Difference => S::Difference,
        BlendMode::Exclusion => S::Exclusion,
        BlendMode::Multiply => S::Multiply,
        BlendMode::Hue => S::Hue,
        BlendMode::Saturation => S::Saturation,
        BlendMode::Color => S::Color,
        BlendMode::Luminosity => S::Luminosity,
    }
}
