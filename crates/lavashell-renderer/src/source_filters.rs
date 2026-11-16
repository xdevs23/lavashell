// SPDX-License-Identifier: GPL-3.0-only

use crate::failure::DrawFailure;
use crate::scalar::scalar;
use lavashell_core::prelude::{Rect, SourceFilter, SourceFilterId};
use skia_safe::ImageFilter;

pub use inventory;

pub struct Registration {
    pub id: SourceFilterId,
    pub build: fn(&[f32], Rect) -> Option<ImageFilter>,
}

inventory::collect!(Registration);

pub(crate) fn resolve(
    filter: &SourceFilter,
    bounds: Rect,
) -> Result<Option<ImageFilter>, DrawFailure> {
    let Some(registration) = inventory::iter::<Registration>
        .into_iter()
        .find(|registration| registration.id == filter.id)
    else {
        return Ok(None);
    };
    let parameters = (filter.parameters.iter())
        .map(|&parameter| scalar(parameter))
        .collect::<Result<Vec<_>, _>>()?;
    Ok((registration.build)(&parameters, bounds))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_error::TestError;
    use crate::test_surfaces::rgba8_pixel;
    use lavashell_core::prelude::*;
    use lavashell_core::retained::{Extent, Footprint};
    use lavashell_core::scene::tree::Packet;
    use skia_safe::AlphaType;

    const OFFSET: SourceFilterId = SourceFilterId(0x7465_7374_6f66_6673);

    inventory::submit! { Registration {
        id: OFFSET,
        build: |parameters, _| skia_safe::image_filters::offset((parameters[0], 0.0), None, None),
    } }

    #[test]
    fn large_registered_filter_outsets_keep_retained_output_and_pixels() -> Result<(), TestError> {
        let bounds = Rect::new(0.dp(), 0.dp(), 16.dp(), 16.dp());
        let packet = Packet::new(
            vec![
                DrawCommand::EffectLayer {
                    bounds,
                    blend: BlendMode::SourceOver,
                    blur: NonNegativeDp::ZERO,
                    filter: Some(SourceFilter {
                        id: OFFSET,
                        parameters: vec![1500.0],
                        outset: constant!(NonNegativeDp::try_new(Dp::new(1500.0))),
                    }),
                },
                DrawCommand::Fill {
                    shape: bounds.into(),
                    brush: Color::WHITE.into(),
                },
                DrawCommand::Restore,
            ]
            .into(),
        )?;
        assert!(matches!(
            packet.footprint().output,
            Some(Extent::Within(output)) if output.x + output.width >= 1516.dp()
        ));
        let tree = crate::test_scenes::leaves([packet])?;
        let mut painter =
            crate::Painter::new(&crate::Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)?;
        let mut surface =
            skia_safe::surfaces::raster_n32_premul((1536, 16)).ok_or(TestError::RasterSurface)?;
        let _canvas_returned_for_chaining = surface.canvas().clear(skia_safe::Color::TRANSPARENT);
        crate::test_scenes::paint(&mut painter, surface.canvas(), &tree, Density::ONE);
        let pixel = rgba8_pixel(&mut surface, (1508, 8), AlphaType::Premul);
        assert_eq!(pixel, [255; 4]);
        Ok(())
    }

    #[test]
    fn nested_registered_offsets_accumulate_until_each_restore() -> Result<(), TestError> {
        let bounds = Rect::new(0.dp(), 0.dp(), 16.dp(), 16.dp());
        let effect = DrawCommand::EffectLayer {
            bounds,
            blend: BlendMode::SourceOver,
            blur: NonNegativeDp::ZERO,
            filter: Some(SourceFilter {
                id: OFFSET,
                parameters: vec![64.0],
                outset: constant!(NonNegativeDp::try_new(Dp::new(64.0))),
            }),
        };
        let packet = Packet::new(
            vec![
                effect.clone(),
                effect,
                DrawCommand::Fill {
                    shape: bounds.into(),
                    brush: Color::WHITE.into(),
                },
                DrawCommand::Restore,
                DrawCommand::Restore,
            ]
            .into(),
        )?;
        assert!(matches!(
            packet.footprint().output,
            Some(Extent::Within(output)) if output.x + output.width >= 144.dp()
        ));
        let tree = crate::test_scenes::leaves([packet])?;
        let mut painter =
            crate::Painter::new(&crate::Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)?;
        let mut surface =
            skia_safe::surfaces::raster_n32_premul((160, 16)).ok_or(TestError::RasterSurface)?;
        let _canvas_returned_for_chaining = surface.canvas().clear(skia_safe::Color::TRANSPARENT);
        crate::test_scenes::paint(&mut painter, surface.canvas(), &tree, Density::ONE);
        let pixel = rgba8_pixel(&mut surface, (136, 8), AlphaType::Premul);
        assert_eq!(pixel, [255; 4]);
        Ok(())
    }

    #[test]
    fn filtered_descendants_survive_unfiltered_and_opacity_groups() -> Result<(), TestError> {
        let bounds = Rect::new(0.dp(), 0.dp(), 16.dp(), 16.dp());
        for (wrapper, expected) in [
            (
                DrawCommand::EffectLayer {
                    bounds,
                    blend: BlendMode::SourceOver,
                    blur: NonNegativeDp::ZERO,
                    filter: None,
                },
                255_u8,
            ),
            (DrawCommand::OpacityLayer { bounds, alpha: 0.5 }, 128_u8),
        ] {
            let commands = vec![
                wrapper,
                DrawCommand::EffectLayer {
                    bounds,
                    blend: BlendMode::SourceOver,
                    blur: NonNegativeDp::ZERO,
                    filter: Some(SourceFilter {
                        id: OFFSET,
                        parameters: vec![64.0],
                        outset: constant!(NonNegativeDp::try_new(Dp::new(64.0))),
                    }),
                },
                DrawCommand::Fill {
                    shape: bounds.into(),
                    brush: Color::WHITE.into(),
                },
                DrawCommand::Restore,
                DrawCommand::Restore,
            ];
            let mut painter =
                crate::Painter::new(&crate::Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)?;
            let mut surface =
                skia_safe::surfaces::raster_n32_premul((96, 16)).ok_or(TestError::RasterSurface)?;
            let _canvas_returned_for_chaining =
                surface.canvas().clear(skia_safe::Color::TRANSPARENT);
            painter.paint_as_leaf(surface.canvas(), &commands, Density::ONE)?;
            let pixel = rgba8_pixel(&mut surface, (72, 8), AlphaType::Premul);
            assert_eq!(pixel, [expected; 4]);
        }
        Ok(())
    }

    #[test]
    fn an_outset_that_overflows_forces_uncached_full_damage() -> Result<(), TestError> {
        let bounds = Rect::new(0.dp(), 0.dp(), 16.dp(), 16.dp());
        let footprint = Footprint::analyze(&[DrawCommand::EffectLayer {
            bounds,
            blend: BlendMode::SourceOver,
            blur: NonNegativeDp::ZERO,
            filter: Some(SourceFilter {
                id: OFFSET,
                parameters: vec![1.0],
                outset: constant!(NonNegativeDp::try_new(Dp::MAX)),
            }),
        }])?;
        assert_eq!(footprint.output, Some(Extent::Everywhere));
        Ok(())
    }
}
