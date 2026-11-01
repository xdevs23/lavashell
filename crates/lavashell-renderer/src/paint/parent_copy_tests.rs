// SPDX-License-Identifier: GPL-3.0-only
use crate::test_error::TestError;
use crate::test_surfaces::{self, pixels};
use crate::{Fonts, Painter};
use lavashell_core::prelude::*;
use skia_safe::Surface;

const WIDTH: u16 = 96;
const HEIGHT: u16 = 48;
const STRIPE: u16 = 4;

/// The colors of the even and the odd stripes of each parent: opaque, translucent, and with an alpha that changes from
/// stripe to stripe.
const PARENTS: [(Color, Color); 3] = [
    (Color::WHITE, Color::rgb(0)),
    (
        Color::WHITE.with_alpha(0.25),
        Color::rgb(0).with_alpha(0.25),
    ),
    (Color::WHITE, Color::WHITE.with_alpha(0.0)),
];

fn dp(pixels: u16) -> Dp {
    Dp::new(f64::from(pixels))
}

fn viewport() -> Rect {
    Rect::new(0.dp(), 0.dp(), dp(WIDTH), dp(HEIGHT))
}

fn stripes((even, odd): (Color, Color)) -> Vec<DrawCommand> {
    (0..WIDTH / STRIPE)
        .map(|stripe| {
            let color = if stripe % 2 == 0 { even } else { odd };
            DrawCommand::Fill {
                shape: Rect::new(dp(stripe * STRIPE), 0.dp(), dp(STRIPE), dp(HEIGHT)).into(),
                brush: color.into(),
            }
        })
        .collect()
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

fn raster() -> Result<Surface, TestError> {
    test_surfaces::surface(i32::from(WIDTH), i32::from(HEIGHT))
}

/// The RGBA bytes of `commands` painted over a transparent `surface`.
fn painted(surface: &mut Surface, commands: &[DrawCommand]) -> Result<Vec<u8>, TestError> {
    let _chained_canvas = surface.canvas().clear(skia_safe::Color::TRANSPARENT);
    let mut painter = Painter::new(&Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)?;
    painter.paint_as_leaf(surface.canvas(), commands, Density::ONE)?;
    Ok(pixels(surface))
}

/// Each image is rounded to 8 bits once, so a channel of `actual` may differ from `expected` by one step.
fn assert_same_image(expected: &[u8], actual: &[u8]) {
    let ((expected, _), (actual, _)) = (expected.as_chunks::<4>(), actual.as_chunks::<4>());
    assert_eq!(expected.len(), actual.len());
    let width = usize::from(WIDTH);
    for (index, (wanted, got)) in expected.iter().zip(actual).enumerate() {
        assert!(
            wanted.iter().zip(got).all(|(w, g)| w.abs_diff(*g) <= 1),
            "pixel ({}, {}): expected {wanted:?}, painted {got:?}",
            index % width,
            index / width
        );
    }
}

fn assert_backdrop_reads_through(surface: &mut Surface) -> Result<(), TestError> {
    for parent in PARENTS {
        let frosted = painted(surface, &[stripes(parent), vec![frost()]].concat())?;
        let layered = [
            stripes(parent),
            vec![
                effect_layer(viewport(), BlendMode::SourceOver, NonNegativeDp::ZERO),
                frost(),
                DrawCommand::Restore,
            ],
        ];
        assert_same_image(&frosted, &painted(surface, &layered.concat())?);
    }
    Ok(())
}

/// A backdrop inside a blended or a blurred layer samples only the empty layer, so the scene does not change.
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
                    effect_layer(viewport(), blend, blur),
                    frost(),
                    DrawCommand::Restore,
                ],
            ];
            assert_same_image(&plain, &painted(surface, &layered.concat())?);
        }
    }
    Ok(())
}

/// The backdrop is larger than the layer bounds, so it samples the scene past them.
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
        );
    }
    Ok(())
}

#[test]
fn a_backdrop_reads_through_only_a_source_over_effect_layer() -> Result<(), TestError> {
    let mut surface = raster()?;
    assert_backdrop_reads_through(&mut surface)?;
    assert_backdrop_reads_only_isolating_layers(&mut surface)
}

#[test]
fn the_bounds_of_an_effect_layer_cut_a_backdrop_that_reads_through_it() -> Result<(), TestError> {
    assert_layer_bounds_cut_the_backdrop(&mut raster()?)
}

#[test]
fn vulkan_backdrop_reads_through_a_source_over_effect_layer() -> Result<(), TestError> {
    let mut gpu = crate::VulkanRenderer::headless()?;
    let mut surface = gpu.offscreen((u32::from(WIDTH), u32::from(HEIGHT)))?;
    assert_backdrop_reads_through(&mut surface)?;
    assert_backdrop_reads_only_isolating_layers(&mut surface)?;
    assert_layer_bounds_cut_the_backdrop(&mut surface)
}
