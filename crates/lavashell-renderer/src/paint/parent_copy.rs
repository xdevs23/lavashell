// SPDX-License-Identifier: GPL-3.0-only
use super::skrect;
use crate::failure::DrawFailure;
use lavashell_core::prelude::*;
use lavashell_core::retained::Extent;
use skia_safe::canvas::{SaveLayerFlags, SaveLayerRec};
use skia_safe::{Canvas, Paint};

pub(super) fn save(canvas: &Canvas, restore: &Paint, bounds: Option<&skia_safe::Rect>) {
    let layer = SaveLayerRec::default()
        .paint(restore)
        .flags(SaveLayerFlags::INIT_WITH_PREVIOUS);
    let _restored_by_the_caller = canvas.save_layer(&match bounds {
        Some(bounds) => layer.bounds(bounds),
        None => layer,
    });
}

pub(super) fn restores_effect_layer(
    blend: BlendMode,
    blur: NonNegativeDp,
    filter: Option<&SourceFilter>,
) -> bool {
    blend == BlendMode::SourceOver && blur == NonNegativeDp::ZERO && filter.is_none()
}

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
    let _paint_returned_for_chaining = restore.set_blend_mode(skia_safe::BlendMode::Src);
    save(canvas, &restore, area.as_ref());
    if let Some(bounds) = bounds {
        let _canvas_returned_for_chaining = canvas.clip_rect(bounds, None, false);
    }
    Ok(())
}
