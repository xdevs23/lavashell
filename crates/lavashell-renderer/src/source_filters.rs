// SPDX-License-Identifier: GPL-3.0-only
//! The registry of source filters. Each implementation of a source filter submits a [`Registration`] with its
//! identifier and a function that builds the Skia image filter from the parameters and the bounds of a draw.

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

/// Builds the Skia image filter for `filter` with the registration whose identifier equals `filter.id`, from the
/// parameters of `filter` converted to Skia scalars. Returns `None` when no registration has that identifier or when
/// the registration builds no filter from those parameters.
///
/// # Errors
/// Fails when a finite parameter of `filter` lies beyond the range of Skia scalars.
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
    use lavashell_core::prelude::*;
    use lavashell_core::retained::{Extent, Footprint};
    use lavashell_core::scene::tree::Packet;

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
        );
        assert!(matches!(
            packet.footprint().output,
            Some(Extent::Within(output)) if output.x + output.width >= 1516.dp()
        ));
        let tree = crate::test_scenes::leaves([packet])?;
        let mut painter =
            crate::Painter::new(&crate::Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)?;
        let mut surface =
            skia_safe::surfaces::raster_n32_premul((1536, 16)).ok_or(TestError::RasterSurface)?;
        let _chained_canvas = surface.canvas().clear(skia_safe::Color::TRANSPARENT);
        crate::test_scenes::paint(&mut painter, surface.canvas(), &tree, Density::ONE);
        let info = skia_safe::ImageInfo::new(
            (1, 1),
            skia_safe::ColorType::RGBA8888,
            skia_safe::AlphaType::Premul,
            None,
        );
        let mut pixel = [0_u8; 4];
        assert!(surface.read_pixels(&info, &mut pixel, 4, (1508, 8)));
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
        );
        assert!(matches!(
            packet.footprint().output,
            Some(Extent::Within(output)) if output.x + output.width >= 144.dp()
        ));
        let tree = crate::test_scenes::leaves([packet])?;
        let mut painter =
            crate::Painter::new(&crate::Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)?;
        let mut surface =
            skia_safe::surfaces::raster_n32_premul((160, 16)).ok_or(TestError::RasterSurface)?;
        let _chained_canvas = surface.canvas().clear(skia_safe::Color::TRANSPARENT);
        crate::test_scenes::paint(&mut painter, surface.canvas(), &tree, Density::ONE);
        let info = skia_safe::ImageInfo::new(
            (1, 1),
            skia_safe::ColorType::RGBA8888,
            skia_safe::AlphaType::Premul,
            None,
        );
        let mut pixel = [0_u8; 4];
        assert!(surface.read_pixels(&info, &mut pixel, 4, (136, 8)));
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
            let _chained_canvas = surface.canvas().clear(skia_safe::Color::TRANSPARENT);
            painter.paint_as_leaf(surface.canvas(), &commands, Density::ONE)?;
            let info = skia_safe::ImageInfo::new(
                (1, 1),
                skia_safe::ColorType::RGBA8888,
                skia_safe::AlphaType::Premul,
                None,
            );
            let mut pixel = [0_u8; 4];
            assert!(surface.read_pixels(&info, &mut pixel, 4, (72, 8)));
            assert_eq!(pixel, [expected; 4]);
        }
        Ok(())
    }

    #[test]
    fn an_outset_that_overflows_forces_uncached_full_damage() {
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
        }]);
        assert_eq!(footprint.output, Some(Extent::Everywhere));
    }
}
