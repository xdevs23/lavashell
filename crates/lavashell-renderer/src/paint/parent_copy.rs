// SPDX-License-Identifier: GPL-3.0-only
use super::skrect;
use crate::failure::DrawFailure;
use lavashell_core::prelude::*;
use lavashell_core::retained::Extent;
use skia_safe::canvas::{SaveLayerFlags, SaveLayerRec};
use skia_safe::{Canvas, Paint};

/// Opens a layer that starts from a copy of the parent and is restored with `restore`, so that a command inside the
/// layer reads the pixels painted below it. The layer covers `bounds`, or the whole clip when `bounds` is None.
pub(super) fn save(canvas: &Canvas, restore: &Paint, bounds: Option<&skia_safe::Rect>) {
    let layer = SaveLayerRec::default()
        .paint(restore)
        .flags(SaveLayerFlags::INIT_WITH_PREVIOUS);
    let _save_count = canvas.save_layer(&match bounds {
        Some(bounds) => layer.bounds(bounds),
        None => layer,
    });
}

/// Whether an effect layer blended with `blend`, blurred by `blur` and filtered by `filter` can start from a copy of
/// the parent. The content of the layer covers the copy, so restoring the layer with Src gives the source-over result.
/// Another blend mode cannot be computed from the copy, and a blur or a filter would also change the copy.
pub(super) fn restores_effect_layer(
    blend: BlendMode,
    blur: NonNegativeDp,
    filter: Option<&SourceFilter>,
) -> bool {
    blend == BlendMode::SourceOver && blur == NonNegativeDp::ZERO && filter.is_none()
}

/// Opens a layer for an effect layer that `restores_effect_layer` accepts. The layer covers `area`, or the whole clip
/// when `area` is None or unbounded. The content is clipped to `bounds`. The layer is restored with Src, so it replaces
/// the parent and a translucent parent is counted once. A failure leaves the canvas as it was.
pub(super) fn save_effect_layer(
    canvas: &Canvas,
    area: Option<Extent>,
    bounds: Option<Rect>,
) -> Result<(), DrawFailure> {
    let area = match area {
        Some(Extent::Within(area)) => Some(skrect(area)?),
        Some(Extent::Everywhere) | None => None,
    };
    let bounds = bounds.map(skrect).transpose()?;
    let mut restore = Paint::default();
    let _chained_paint = restore.set_blend_mode(skia_safe::BlendMode::Src);
    save(canvas, &restore, area.as_ref());
    if let Some(bounds) = bounds {
        let _chained_canvas = canvas.clip_rect(bounds, None, false);
    }
    Ok(())
}
