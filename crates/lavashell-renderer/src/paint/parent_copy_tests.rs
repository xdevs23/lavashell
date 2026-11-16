// SPDX-License-Identifier: GPL-3.0-only
use crate::test_error::TestError;
use crate::test_surfaces::{EFFECT_STRIPES, EFFECT_SURFACE, Pixels, channel_differences, pixels};
use crate::{Fonts, Painter};
use lavashell_core::prelude::*;
use skia_safe::Surface;

const PARENTS: [(Color, Color); 3] = [
    (Color::WHITE, Color::rgb(0)),
    (
        Color::WHITE.with_alpha(0.25),
        Color::rgb(0).with_alpha(0.25),
    ),
    (Color::WHITE, Color::WHITE.with_alpha(0.0)),
];

fn stripes((even, odd): (Color, Color)) -> Vec<DrawCommand> {
    EFFECT_STRIPES.stripe_fills([even, odd])
}

fn frost() -> DrawCommand {
    DrawCommand::Backdrop {
        shape: Rect::new(16.dp(), 4.dp(), 64.dp(), 40.dp()).into(),
        effect: constant!(Backdrop::blur(Dp::new(5.0))),
    }
}

fn effect_layer(bounds: Rect, blend: BlendMode, blur: NonNegativeDp) -> DrawCommand {
    DrawCommand::EffectLayer {
        bounds,
        blend,
        blur,
        filter: None,
    }
}

fn painted(surface: &mut Surface, commands: &[DrawCommand]) -> Result<Pixels<[u8; 4]>, TestError> {
    let _canvas_returned_for_chaining = surface.canvas().clear(skia_safe::Color::TRANSPARENT);
    let mut painter = Painter::new(&Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)?;
    painter.paint_as_leaf(surface.canvas(), commands, Density::ONE)?;
    Ok(pixels(surface))
}

fn assert_same_image(
    expected: &Pixels<[u8; 4]>,
    actual: &Pixels<[u8; 4]>,
) -> Result<(), TestError> {
    for ((x, y), &(wanted, got)) in expected.zipped(actual)?.positioned()? {
        assert!(
            channel_differences(wanted, got).all(|difference| difference <= 1),
            "pixel ({x}, {y}): expected {wanted:?}, painted {got:?}"
        );
    }
    Ok(())
}

fn assert_backdrop_reads_through(surface: &mut Surface) -> Result<(), TestError> {
    for parent in PARENTS {
        let frosted = painted(surface, &[stripes(parent), vec![frost()]].concat())?;
        let layered = [
            stripes(parent),
            vec![
                effect_layer(
                    EFFECT_SURFACE.bounds(),
                    BlendMode::SourceOver,
                    NonNegativeDp::ZERO,
                ),
                frost(),
                DrawCommand::Restore,
            ],
        ];
        assert_same_image(&frosted, &painted(surface, &layered.concat())?)?;
    }
    Ok(())
}

fn assert_backdrop_reads_only_isolating_layers(surface: &mut Surface) -> Result<(), TestError> {
    let isolating = [
        (BlendMode::Multiply, NonNegativeDp::ZERO),
        (
            BlendMode::SourceOver,
            constant!(NonNegativeDp::try_new(Dp::new(2.0))),
        ),
    ];
    for parent in PARENTS {
        let plain = painted(surface, &stripes(parent))?;
        for (blend, blur) in isolating {
            let layered = [
                stripes(parent),
                vec![
                    effect_layer(EFFECT_SURFACE.bounds(), blend, blur),
                    frost(),
                    DrawCommand::Restore,
                ],
            ];
            assert_same_image(&plain, &painted(surface, &layered.concat())?)?;
        }
    }
    Ok(())
}

fn assert_layer_bounds_cut_the_backdrop(surface: &mut Surface) -> Result<(), TestError> {
    let bounds = Rect::new(24.dp(), 8.dp(), 48.dp(), 32.dp());
    for parent in PARENTS {
        let clipped = [
            stripes(parent),
            vec![
                DrawCommand::Save,
                DrawCommand::Clip {
                    rect: bounds,
                    corners: Corners::default(),
                },
                frost(),
                DrawCommand::Restore,
            ],
        ];
        let layered = [
            stripes(parent),
            vec![
                effect_layer(bounds, BlendMode::SourceOver, NonNegativeDp::ZERO),
                frost(),
                DrawCommand::Restore,
            ],
        ];
        assert_same_image(
            &painted(surface, &clipped.concat())?,
            &painted(surface, &layered.concat())?,
        )?;
    }
    Ok(())
}

#[test]
fn a_backdrop_reads_through_only_a_source_over_effect_layer() -> Result<(), TestError> {
    let mut surface = EFFECT_SURFACE.raster()?;
    assert_backdrop_reads_through(&mut surface)?;
    assert_backdrop_reads_only_isolating_layers(&mut surface)
}

#[test]
fn the_bounds_of_an_effect_layer_cut_a_backdrop_that_reads_through_it() -> Result<(), TestError> {
    assert_layer_bounds_cut_the_backdrop(&mut EFFECT_SURFACE.raster()?)
}

#[test]
fn vulkan_backdrop_reads_through_a_source_over_effect_layer() -> Result<(), TestError> {
    let mut gpu = crate::VulkanRenderer::headless()?;
    let mut surface = gpu.offscreen(EFFECT_SURFACE.pixel_size())?;
    assert_backdrop_reads_through(&mut surface)?;
    assert_backdrop_reads_only_isolating_layers(&mut surface)?;
    assert_layer_bounds_cut_the_backdrop(&mut surface)
}
