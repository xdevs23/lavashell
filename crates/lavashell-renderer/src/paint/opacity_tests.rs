// SPDX-License-Identifier: GPL-3.0-only
use crate::failure::DrawFailure;
use crate::test_error::TestError;
use crate::test_surfaces::{self, ChannelRounding};
use crate::{Fonts, Painter};
use lavashell_core::prelude::*;
use lavashell_core::scene::tree::{
    Change, Content, Group, Node, Packet, SceneNodeId, SceneTree, SceneUpdate, Slot,
};
use skia_safe::{AlphaType, ColorType, ImageInfo, Surface};
use std::ops::DerefMut;
use std::sync::Arc;

fn surface() -> Result<Surface, TestError> {
    test_surfaces::surface(96, 48)
}

fn pixel(surface: &mut Surface, x: i32, y: i32) -> [f64; 4] {
    let info = ImageInfo::new(
        (1, 1),
        ColorType::RGBAF32,
        AlphaType::Premul,
        skia_safe::ColorSpace::new_srgb_linear(),
    );
    let mut bytes = [0u8; 16];
    assert!(surface.read_pixels(&info, &mut bytes, 16, (x, y)));
    let (floats, _) = bytes.as_chunks::<4>();
    std::array::from_fn(|i| f64::from(f32::from_ne_bytes(floats[i])))
}

fn commands(opacity: f64, background_alpha: f64) -> Vec<DrawCommand> {
    let mut commands = Vec::new();
    let context = DrawContext::new(Viewport::new(Size::new(96.dp(), 48.dp()), Density::ONE));
    let mut drawing = Drawing::new(
        &mut commands,
        Rect::new(0.dp(), 0.dp(), 96.dp(), 48.dp()),
        &context,
    );
    for stripe in 0..24_u16 {
        drawing.fill(
            Rect::new(4.dp() * f64::from(stripe), 0.dp(), 4.dp(), 48.dp()),
            if stripe % 2 == 0 {
                Color::WHITE.with_alpha(background_alpha)
            } else {
                Color::rgb(0).with_alpha(background_alpha)
            },
        );
    }
    drawing.commands.push(DrawCommand::OpacityLayer {
        alpha: opacity,
        bounds: Rect::new(12.dp(), 0.dp(), 72.dp(), 48.dp()),
    });
    drawing.backdrop(
        Shape::rounded(Rect::new(16.dp(), 4.dp(), 64.dp(), 40.dp()), 8.dp()),
        constant!(Backdrop::blur(Dp::new(5.0))),
    );
    drawing.fill(
        Rect::new(40.dp(), 20.dp(), 4.dp(), 4.dp()),
        Color::rgb(0x00ff_0000),
    );
    drawing.restore();
    commands
}

fn faded(
    below: Vec<DrawCommand>,
    alpha: f64,
    inside: Vec<DrawCommand>,
) -> Result<SceneTree, TestError> {
    let leaf =
        |commands: Vec<DrawCommand>| Node::Leaf(Content::Packet(Packet::new(Arc::from(commands))));
    let group = Group {
        opacity: Slot::Value(alpha),
        ..Group::default()
    };
    let insert = |id, parent: Option<u64>, index, node| Change::Insert {
        id: SceneNodeId(id),
        parent: parent.map(SceneNodeId),
        index,
        node,
    };
    let changes = vec![
        insert(1, None, 0, leaf(below)),
        insert(2, None, 1, Node::Group(group)),
        insert(3, Some(2), 0, leaf(inside)),
    ];
    let mut tree = SceneTree::default();
    tree.show(SceneUpdate { changes })?;
    Ok(tree)
}

fn assert_first_frame_blur(surface: &mut Surface) -> Result<(), TestError> {
    let mut painter = Painter::new(&Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)?;
    for background_alpha in [1.0, 0.25] {
        for alpha in [0.1, 0.5, 0.9, 1.0] {
            let _chained_canvas = surface.canvas().clear(skia_safe::Color::TRANSPARENT);
            painter.paint_as_leaf(
                surface.canvas(),
                &commands(alpha, background_alpha),
                Density::ONE,
            )?;
            let blurred_black_stripe = pixel(surface, 38, 12);
            assert!(
                (blurred_black_stripe[0] - alpha * background_alpha * 0.5).abs() < 0.01,
                "first frame, opacity {alpha}, background {background_alpha}: {blurred_black_stripe:?}"
            );
            let rounding = ChannelRounding::of(surface)?;
            // Inside the layer, a pixel is rounded three times: by the stripes, by the blurred copy or the square in
            // the layer, and by the composite of the layer.
            let layered = |expected| rounding.after(3, expected);
            assert!(
                (blurred_black_stripe[3] - background_alpha).abs() <= layered(background_alpha),
                "backdrop changed parent alpha: {blurred_black_stripe:?}"
            );
            let foreground = pixel(surface, 41, 21);
            let below = (1.0 - alpha) * background_alpha;
            let expected = [alpha + below, below, below, alpha + below];
            for (channel, expected) in foreground.into_iter().zip(expected) {
                assert!(
                    (channel - expected).abs() <= layered(expected),
                    "{foreground:?}"
                );
            }
            let white = pixel(surface, 2, 24)[0];
            let unlayered = |expected| rounding.after(1, expected);
            assert!(
                (white - background_alpha).abs() <= unlayered(background_alpha),
                "{white}"
            );
            let black = pixel(surface, 6, 24)[0];
            assert!(black.abs() <= unlayered(0.0), "{black}");
        }
    }
    Ok(())
}

#[test]
fn backdrop_blurs_on_the_first_partial_opacity_frame_without_doubling_parent_alpha()
-> Result<(), TestError> {
    assert_first_frame_blur(&mut surface()?)
}

#[test]
fn vulkan_backdrop_blurs_on_the_first_partial_opacity_frame() -> Result<(), TestError> {
    let mut gpu = crate::VulkanRenderer::headless()?;
    assert_first_frame_blur(&mut *gpu.offscreen((96, 48))?)?;
    assert_nested_groups(|| Ok(gpu.offscreen((96, 48))?))
}

#[test]
fn nested_fading_retained_groups_preserve_hdr_and_blur_sampling_outside_panel_bounds()
-> Result<(), TestError> {
    assert_nested_groups(|| Ok(Box::new(surface()?)))
}

fn assert_nested_groups<S: DerefMut<Target = Surface>>(
    mut create: impl FnMut() -> Result<S, TestError>,
) -> Result<(), TestError> {
    let rect = Rect::new(16.dp(), 4.dp(), 64.dp(), 40.dp());
    let (headroom, background_alpha) = (3.0, 0.25);
    let mut render = |alpha| -> Result<S, TestError> {
        let mut surface = create()?;
        let _chained_canvas = surface.canvas().clear(skia_safe::Color::TRANSPARENT);
        let mut commands = commands(1.0, background_alpha);
        commands.truncate(24);
        for command in &mut commands {
            if let DrawCommand::Fill { brush, .. } = command
                && let Brush::Solid(color) = brush
                && color.to_linear(Gamut::Srgb)[0] > 0.5
            {
                *color = Color::linear([headroom; 3], background_alpha, Gamut::Srgb);
            }
        }
        let child = vec![
            DrawCommand::Opacity(0.5),
            DrawCommand::Backdrop {
                shape: Shape::rounded(rect, 10.dp()),
                effect: constant!(Backdrop::blur(Dp::new(9.0))),
            },
            DrawCommand::Restore,
        ];
        let tree = faded(commands, alpha, child)?;
        let mut painter = Painter::new(&Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)?;
        crate::test_scenes::paint(&mut painter, surface.canvas(), &tree, Density::ONE);
        Ok(surface)
    };
    let mut parent = render(0.0)?;
    let mut result = render(1.0)?;
    let mut half = render(0.5)?;
    let rounding = ChannelRounding::of(&half)?;
    // A render rounds a pixel up to four times: the stripes, the blurred copy, the layer of the child and the group.
    // The midpoint adds the rounding of all three renders, two of them at half their weight.
    let midpoint_rounding = |expected| rounding.after(8, expected);
    for (x, y) in [(16, 24), (38, 12), (17, 5), (2, 24)] {
        let (a, b, c) = (
            pixel(&mut parent, x, y),
            pixel(&mut result, x, y),
            pixel(&mut half, x, y),
        );
        for channel in 0..4 {
            let expected = f64::midpoint(a[channel], b[channel]);
            assert!(
                (c[channel] - expected).abs() <= midpoint_rounding(expected),
                "{a:?} {b:?} {c:?}"
            );
        }
        assert!(
            (c[3] - background_alpha).abs() <= rounding.after(4, background_alpha),
            "parent alpha duplicated"
        );
    }
    // The stripes outside the panel are rounded once.
    let headroom_shown = headroom * background_alpha;
    assert!(
        (pixel(&mut half, 2, 24)[0] - headroom_shown).abs() <= rounding.after(1, headroom_shown),
        "HDR headroom was clamped"
    );
    assert!(
        (pixel(&mut result, 16, 24)[0] - 0.5625).abs() < 0.012,
        "blur halo lost parent samples"
    );
    Ok(())
}

#[test]
fn independently_registered_destination_reader_blurs_through_opacity() -> Result<(), TestError> {
    assert_independent_reader(false)
}

#[test]
fn independently_registered_retained_reader_resamples_changing_parent() -> Result<(), TestError> {
    assert_independent_reader(true)
}

fn assert_independent_reader(retained: bool) -> Result<(), TestError> {
    use crate::effect_registry::{DrawTarget, EffectBackend, EffectRegistry, EffectRequest};
    use lavashell_effect_quality::{EffectId, VariantId};
    use skia_safe::{Canvas, Paint, canvas::SaveLayerRec, image_filters};
    struct Reader;
    impl Reader {
        fn blur_backdrop(canvas: &Canvas) -> Result<(), DrawFailure> {
            let filter = image_filters::blur((5.0, 5.0), skia_safe::TileMode::Clamp, None, None)
                .ok_or(DrawFailure::ShaderRejected)?;
            let mut paint = Paint::default();
            let _chained_paint = paint.set_blend_mode(skia_safe::BlendMode::Src);
            let _save_count =
                canvas.save_layer(&SaveLayerRec::default().paint(&paint).backdrop(&filter));
            let _chained_canvas = canvas.restore();
            Ok(())
        }
    }
    impl EffectBackend for Reader {
        fn variants(&self, _: &DrawCommand, _: DrawTarget) -> Vec<VariantId> {
            vec![VariantId(1)]
        }

        fn reads_destination(&self) -> bool {
            true
        }
        fn render(&mut self, canvas: &Canvas, request: &EffectRequest<'_>) {
            if let Err(failure) = Self::blur_backdrop(canvas) {
                request.painter.record(failure);
            }
        }
    }
    let mut painter = Painter::new(&Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)?;
    painter.effect_registry = EffectRegistry::empty();
    let id = EffectId(0x1234);
    painter.effect_registry_mut().register_effect(
        id,
        |c| matches!(c, DrawCommand::Circle { .. }),
        Reader,
    )?;
    let mut commands = commands(0.5, 1.0);
    commands[25] = DrawCommand::Circle {
        center: Point::new(48.dp(), 24.dp()),
        radius: 8.dp(),
        brush: Color::WHITE.into(),
    };
    let mut surface = surface()?;
    for pass in 0..4 {
        let _chained_canvas = surface.canvas().clear(skia_safe::Color::TRANSPARENT);
        if retained {
            let inside = commands[25..27].to_vec();
            let tree = faded(commands[..24].to_vec(), 0.5, inside)?;
            crate::test_scenes::paint(&mut painter, surface.canvas(), &tree, Density::ONE);
        } else {
            painter.paint_as_leaf(surface.canvas(), &commands, Density::ONE)?;
        }
        let expected = if pass < 3 { 0.25 } else { 0.75 };
        let actual = pixel(&mut surface, 38, 12)[0];
        assert!(
            (actual - expected).abs() < 0.005,
            "pass {pass}: {actual}, expected {expected}"
        );
        if pass == 2 {
            for command in &mut commands[..24] {
                if let DrawCommand::Fill { brush, .. } = command
                    && let Brush::Solid(color) = brush
                {
                    *color = if color.to_linear(Gamut::Srgb)[0] > 0.5 {
                        Color::rgb(0)
                    } else {
                        Color::WHITE
                    };
                }
            }
        }
    }
    assert_eq!(painter.render_stats().cache_bytes, 0);
    Ok(())
}
