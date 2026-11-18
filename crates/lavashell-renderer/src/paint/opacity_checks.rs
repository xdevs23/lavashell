// SPDX-License-Identifier: GPL-3.0-only
use crate::test_error::TestError;
use crate::test_surfaces::{ChannelRounding, EFFECT_STRIPES, EFFECT_SURFACE, linear_float_pixel};
use crate::{Fonts, Painter};
use lavashell_core::prelude::*;
use lavashell_core::retained::UnbalancedRestore;
use lavashell_core::scene::tree::{
    Change, Content, Group, Node, Packet, SceneNodeId, SceneTree, Slot,
};
use lavashell_test_fixtures::show_all;
use skia_safe::{AlphaType, Surface};
use std::ops::DerefMut;
use std::sync::Arc;

pub fn pixel(surface: &mut Surface, x: i32, y: i32) -> [f64; 4] {
    linear_float_pixel(surface, x, y, AlphaType::Premul).map(f64::from)
}

#[must_use]
pub fn commands(opacity: f64, background_alpha: f64) -> Vec<DrawCommand> {
    let mut commands = Vec::new();
    let context = DrawContext::new(Viewport::new(EFFECT_SURFACE.size(), Density::ONE));
    let mut drawing = Drawing::new(&mut commands, EFFECT_SURFACE.bounds(), &context);
    EFFECT_STRIPES.fill_stripes(
        &mut drawing,
        [Color::WHITE, Color::rgb(0)].map(|color| color.with_alpha(background_alpha)),
    );
    drawing.commands.push(DrawCommand::OpacityLayer {
        alpha: opacity,
        bounds: Rect::new(12.dp(), 0.dp(), 72.dp(), EFFECT_SURFACE.size().height),
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

pub fn faded(
    below: Vec<DrawCommand>,
    alpha: f64,
    inside: Vec<DrawCommand>,
) -> Result<SceneSnapshot, TestError> {
    let leaf = |commands: Vec<DrawCommand>| -> Result<Node, UnbalancedRestore> {
        Ok(Node::Leaf(Content::Packet(Packet::new(Arc::from(
            commands,
        ))?)))
    };
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
    let changes = [
        insert(1, None, 0, leaf(below)?),
        insert(2, None, 1, Node::Group(group)),
        insert(3, Some(2), 0, leaf(inside)?),
    ];
    let mut tree = SceneTree::default();
    show_all(&mut tree, changes)?;
    Ok(tree.snapshot())
}

pub fn assert_first_frame_blur(surface: &mut Surface) -> Result<(), TestError> {
    let mut painter = Painter::new(&Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)?;
    for background_alpha in [1.0, 0.25] {
        for alpha in [0.1, 0.5, 0.9, 1.0] {
            let _canvas_returned_for_chaining =
                surface.canvas().clear(skia_safe::Color::TRANSPARENT);
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

pub fn assert_nested_groups<S: DerefMut<Target = Surface>, E: From<TestError>>(
    mut create: impl FnMut() -> Result<S, E>,
) -> Result<(), E> {
    let rect = Rect::new(16.dp(), 4.dp(), 64.dp(), 40.dp());
    let (headroom, background_alpha) = (3.0, 0.25);
    let mut render = |alpha| -> Result<S, E> {
        let mut surface = create()?;
        let _canvas_returned_for_chaining = surface.canvas().clear(skia_safe::Color::TRANSPARENT);
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
        let mut painter = Painter::new(&Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)
            .map_err(TestError::from)?;
        crate::test_scenes::paint(&mut painter, surface.canvas(), &tree, Density::ONE)
            .map_err(TestError::from)?;
        Ok(surface)
    };
    let mut parent = render(0.0)?;
    let mut result = render(1.0)?;
    let mut half = render(0.5)?;
    let rounding = ChannelRounding::of(&half)?;
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
