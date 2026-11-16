// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]

use lavashell_core::prelude::*;
use lavashell_core::scene::tree::ApplyError;
use lavashell_core::surface::SurfaceIds;
use lavashell_effect_quality::{EffectId, OfferedVariant, VariantId, VariantQuality};
use lavashell_renderer::effect_registry::{
    DrawTarget, EffectBackend, EffectRegistration, EffectRegistry, EffectRequest,
};
use lavashell_renderer::{Error, FieldRoom, Fonts, Painter, UniformRange, skia};
use std::time::Duration;

const ID: EffectId = EffectId(0x00ab_cdef);

const ONLY_VARIANT: OfferedVariant = OfferedVariant {
    variant: VariantId(1),
    quality: VariantQuality(0),
};

inventory::submit! { EffectRegistration { install } }

fn install(registry: &mut EffectRegistry, _: &FieldRoom) -> Result<(), Error> {
    registry.register_effect(ID, |c| matches!(c, DrawCommand::Shadow { .. }), GreenFill)
}

#[derive(Debug, thiserror::Error)]
enum RenderError {
    #[error("cannot start the painter")]
    Painter(#[source] Error),
    #[error("cannot allocate the surface")]
    Surface,
    #[error("cannot build the scene")]
    Scene(#[source] ApplyError),
    #[error("cannot copy the surface into a raster image")]
    RasterImage,
    #[error("cannot read the pixels of the raster image")]
    Pixels,
    #[error("the surface of the probe could not get an id")]
    NumberSurface(#[source] lavashell_core::id_counter::IdsExhausted),
}

struct GreenFill;

impl EffectBackend for GreenFill {
    fn variants(&self, _: &DrawCommand, _: DrawTarget) -> Vec<OfferedVariant> {
        vec![ONLY_VARIANT]
    }

    fn render(&mut self, canvas: &skia::Canvas, _: &EffectRequest<'_>) {
        let _canvas_returned_for_chaining = canvas.clear(skia::Color::GREEN);
    }
}

fn main() -> Result<(), RenderError> {
    let rect = Rect::new(0.dp(), 0.dp(), 8.dp(), 8.dp());
    let context = DrawContext::new(Viewport::new(Size::new(8.dp(), 8.dp()), Density::ONE));
    let mut commands = Vec::new();
    let mut drawing = Drawing::new(&mut commands, rect, &context);
    drawing.backdrop(rect, constant!(Backdrop::blur(Dp::new(4.0))));
    drawing.shadow(
        rect,
        constant!(Shadow::new(
            Color::WHITE,
            Dp::new(1.0),
            Point::new(Dp::ZERO, Dp::ZERO),
        )),
    );
    let mut painter =
        Painter::new(&Fonts::new(), UniformRange::VULKAN_REQUIRED).map_err(RenderError::Painter)?;
    let registry = painter.effect_registry_mut();
    assert!(registry.identify(&commands[0]).is_none());
    assert_eq!(registry.identify(&commands[1]), Some(ID));
    let target = DrawTarget {
        pixels_per_dp: Density::ONE.get(),
    };
    assert_eq!(registry.variants(ID, &commands[1], target), [ONLY_VARIANT]);
    let mut surface = skia::surfaces::raster_n32_premul((8, 8)).ok_or(RenderError::Surface)?;
    let tree = SceneTree::with_leaf(commands).map_err(RenderError::Scene)?;
    let probe_surface = SurfaceIds::default()
        .next_id()
        .map_err(RenderError::NumberSurface)?;
    painter.paint_scene(
        surface.canvas(),
        probe_surface,
        tree.at(Duration::ZERO),
        Density::ONE,
    );
    let image =
        (surface.image_snapshot().make_raster_image(None, None)).ok_or(RenderError::RasterImage)?;
    let pixels = image.peek_pixels().ok_or(RenderError::Pixels)?;
    assert_eq!(pixels.get_color((4, 4)), skia::Color::GREEN);
    Ok(())
}
