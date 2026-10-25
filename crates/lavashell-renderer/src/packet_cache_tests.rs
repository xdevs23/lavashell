// SPDX-License-Identifier: GPL-3.0-only
use crate::effect_registry::{DrawTarget, EffectBackend, EffectRequest};
use crate::test_error::TestError;
use crate::test_scenes::insert;
use crate::{BlendSpace, Error, Fonts, Painter, SurfaceFrame};
use lavashell_core::prelude::*;
use lavashell_core::scene::tree::*;
use lavashell_effect_quality::{EffectId, VariantId};
use skia_safe::{AlphaType, Canvas, ColorType, ImageInfo};
use std::{sync::Arc, time::Duration};

const WIDTH: i32 = 120;
const HEIGHT: i32 = 80;

fn fill(rect: Rect, color: Color) -> DrawCommand {
    DrawCommand::Fill {
        shape: rect.into(),
        brush: color.into(),
    }
}

fn text(rect: Rect) -> DrawCommand {
    let mut style =
        TextStyle::new("sans-serif", 13.sp(), Color::rgb(0x0020_2830)).resolve_unscaled();
    style.max_lines = Some(1);
    DrawCommand::Text {
        text: "Lavashell 18:42".into(),
        rect,
        style,
    }
}

fn stripes() -> Vec<DrawCommand> {
    (0..12_u8)
        .map(|x| {
            let rect = Rect::new(10.dp() * f64::from(x), 0.dp(), 10.dp(), 80.dp());
            match x % 2 {
                0 => fill(rect, Color::rgb(0x002a_4d8f)),
                _ => fill(rect, Color::rgb(0x00f0_c060).with_alpha(0.6)),
            }
        })
        .collect()
}

const CARD: Rect = Rect::new(Dp::ZERO, Dp::ZERO, Dp::new(64.0), Dp::new(40.0));

fn card() -> Vec<DrawCommand> {
    vec![
        fill(CARD, Color::rgb(0x00fa_fafa)),
        text(Rect::new(6.dp(), 4.dp(), 52.dp(), 18.dp())),
        DrawCommand::Circle {
            center: Point::new(48.dp(), 28.dp()),
            radius: 7.dp(),
            brush: Color::rgb(0x00d0_4020).with_alpha(0.7).into(),
        },
    ]
}

fn leaf(commands: Vec<DrawCommand>) -> Node {
    Node::Leaf(Content::Packet(Packet::new(Arc::from(commands))))
}

fn scene(
    place: Point,
    clip: Option<RoundedRect>,
    commands: Vec<DrawCommand>,
) -> Result<SceneTree, TestError> {
    let group = Group {
        transform: Slot::Value(Affine::translate(place)),
        opacity: Slot::Value(1.0),
        clip: clip.map(Slot::Value),
    };
    let mut tree = SceneTree::default();
    tree.show(SceneUpdate::from_iter([
        insert(1, None, 0, leaf(stripes())),
        insert(2, None, 1, Node::Group(group)),
        insert(3, Some(2), 0, leaf(commands)),
    ]))?;
    Ok(tree)
}

fn at(x: f64, y: f64) -> Point {
    Point::new(Dp::new(x), Dp::new(y))
}

fn painted(
    color: ColorType,
    paint: impl FnOnce(&Canvas) -> Result<(), TestError>,
) -> Result<Vec<u8>, TestError> {
    let info = ImageInfo::new(
        (WIDTH, HEIGHT),
        color,
        AlphaType::Premul,
        crate::color::working_color_space().ok_or(Error::WorkingColorSpace)?,
    );
    let mut surface =
        skia_safe::surfaces::raster(&info, None, None).ok_or(TestError::RasterSurface)?;
    let _chained_canvas = surface.canvas().clear(skia_safe::Color::TRANSPARENT);
    paint(surface.canvas())?;
    let mut bytes = vec![0; info.compute_min_byte_size()];
    assert!(surface.read_pixels(&info, &mut bytes, info.min_row_bytes(), (0, 0)));
    Ok(bytes)
}

/// Paints three times and compares each paint with a direct paint. The first paint stores no copy, the second fills
/// the copy and the third draws the copy.
fn cache_hits(
    color: ColorType,
    prepare: impl Fn(&mut Painter) -> Result<(), TestError>,
    paint: impl Fn(&mut Painter, &Canvas) -> Result<(), TestError>,
) -> Result<u64, TestError> {
    let painter = || -> Result<Painter, TestError> {
        let mut painter = Painter::new(&Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)?;
        prepare(&mut painter)?;
        Ok(painter)
    };
    let mut direct = painter()?;
    direct.set_packet_caching(crate::PacketCaching::Disabled);
    let expected = painted(color, |canvas| paint(&mut direct, canvas))?;
    let mut cached = painter()?;
    for frame in 0..3 {
        let actual = painted(color, |canvas| paint(&mut cached, canvas))?;
        let changed = actual.iter().zip(&expected).filter(|(a, b)| a != b).count();
        assert_eq!(changed, 0, "bytes changed in frame {frame}");
    }
    Ok(cached.render_stats().cache_hits)
}

fn scene_hits(tree: &SceneTree, density: Density) -> Result<u64, TestError> {
    cache_hits(ColorType::RGBAF16, |_| Ok(()), paint_scene(tree, density))
}

fn paint_scene(
    tree: &SceneTree,
    density: Density,
) -> impl Fn(&mut Painter, &Canvas) -> Result<(), TestError> {
    move |painter, canvas| {
        painter.paint_scene(canvas, tree.at(Duration::ZERO), density);
        Ok(())
    }
}

#[test]
fn the_text_of_the_card_paints_glyphs() -> Result<(), TestError> {
    let blank: Vec<_> = card()
        .into_iter()
        .filter(|command| !matches!(command, DrawCommand::Text { .. }))
        .collect();
    let paint = |commands| -> Result<Vec<u8>, TestError> {
        let tree = scene(at(12.0, 8.0), None, commands)?;
        let mut painter = Painter::new(&Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)?;
        painted(ColorType::RGBAF16, |canvas| {
            paint_scene(&tree, Density::ONE)(&mut painter, canvas)
        })
    };
    assert_ne!(paint(card())?, paint(blank)?);
    Ok(())
}

#[test]
fn an_opaque_card_on_whole_pixels_composites_to_its_painted_pixels() -> Result<(), TestError> {
    let dense = Density::from_scale(1.5).ok_or(TestError::NoDensity(1.5))?;
    for (place, density) in [(at(12.0, 8.0), Density::ONE), (at(8.0, 6.0), dense)] {
        let hits = scene_hits(&scene(place, None, card())?, density)?;
        assert_eq!(hits, 1, "{place:?} at {density:?}");
    }
    Ok(())
}

#[test]
fn a_card_at_a_fraction_of_a_device_pixel_paints_directly() -> Result<(), TestError> {
    for phase in [0.25, 0.5] {
        let place = at(12.0 + phase, 8.0 + phase);
        assert_eq!(scene_hits(&scene(place, None, card())?, Density::ONE)?, 0);
    }
    Ok(())
}

#[test]
fn leaves_without_an_opaque_fill_below_all_they_paint_paint_directly() -> Result<(), TestError> {
    let label = text(Rect::new(6.dp(), 4.dp(), 52.dp(), 18.dp()));
    let glass = vec![
        DrawCommand::Backdrop {
            shape: Shape::rounded(CARD, 10.dp()),
            effect: constant!(Backdrop::glass(
                Dp::new(4.0),
                Glass {
                    refraction: constant!(NonNegativeDp::try_new(Dp::new(12.0))),
                    dispersion: constant!(NonNegativeDp::try_new(Dp::new(0.7))),
                    rim: constant!(NonNegative::try_new(0.14)),
                },
            )),
        },
        fill(CARD, Color::WHITE.with_alpha(0.2)),
    ];
    let rounded = DrawCommand::Fill {
        shape: Shape::rounded(CARD, 10.dp()),
        brush: Color::rgb(0x00fa_fafa).into(),
    };
    let translucent = fill(CARD, Color::rgb(0x0040_80c0).with_alpha(0.5));
    for (name, commands) in [
        ("text", vec![label.clone()]),
        ("glass", glass),
        ("rounded", vec![rounded, label.clone()]),
        ("translucent", vec![translucent, label]),
    ] {
        for phase in [0.0, 0.25, 0.5] {
            let tree = scene(at(12.0 + phase, 8.0 + phase), None, commands.clone())?;
            assert_eq!(scene_hits(&tree, Density::ONE)?, 0, "{name} at {phase}");
        }
    }
    Ok(())
}

#[test]
fn a_card_whose_height_alone_scales_paints_directly() -> Result<(), TestError> {
    let mut tree = scene(at(12.0, 8.0), None, card())?;
    let mut cached = Painter::new(&Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)?;
    let paint = |painter: &mut Painter, tree: &SceneTree| {
        painted(ColorType::RGBAF16, |canvas| {
            paint_scene(tree, Density::ONE)(painter, canvas)
        })
    };
    let first = paint(&mut cached, &tree)?;
    for _ in 1..3 {
        assert_eq!(paint(&mut cached, &tree)?, first);
    }
    assert_eq!(cached.render_stats().cache_hits, 1);
    let squeezed = Affine::scale(1.0, 0.75).translated(at(12.0, 8.0));
    tree.show(SceneUpdate::from_iter([Change::SetSlot {
        id: SceneNodeId(2),
        slot: SlotChange::Transform(Slot::Value(squeezed)),
    }]))?;
    let mut direct = Painter::new(&Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)?;
    direct.set_packet_caching(crate::PacketCaching::Disabled);
    assert_eq!(paint(&mut cached, &tree)?, paint(&mut direct, &tree)?);
    assert_eq!(cached.render_stats().cache_hits, 1);
    Ok(())
}

fn clip(rect: Rect, radius: f64) -> RoundedRect {
    RoundedRect::new(rect, Corners::all(Dp::new(radius)))
}

fn cutting(radius: f64) -> RoundedRect {
    clip(Rect::new(0.dp(), 0.dp(), 40.dp(), 30.dp()), radius)
}

fn around() -> RoundedRect {
    clip(Rect::new((-4).dp(), (-4).dp(), 72.dp(), 48.dp()), 4.0)
}

#[test]
fn a_card_is_copied_into_an_image_only_inside_its_clip() -> Result<(), TestError> {
    for (clip, expected) in [(cutting(8.0), 0), (cutting(0.0), 0), (around(), 1)] {
        let tree = scene(at(12.0, 8.0), Some(clip), card())?;
        assert_eq!(scene_hits(&tree, Density::ONE)?, expected, "{clip:?}");
    }
    Ok(())
}

#[test]
fn a_card_image_stands_in_under_a_clip_cutting_it_only_with_hard_edges() -> Result<(), TestError> {
    let mut tree = scene(at(12.0, 8.0), Some(around()), card())?;
    let mut cached = Painter::new(&Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)?;
    let paint = |painter: &mut Painter, tree: &SceneTree| {
        painted(ColorType::RGBAF16, |canvas| {
            paint_scene(tree, Density::ONE)(painter, canvas)
        })
    };
    let first = paint(&mut cached, &tree)?;
    assert_eq!(paint(&mut cached, &tree)?, first);
    for (clip, hits) in [(cutting(8.0), 0), (cutting(0.0), 1)] {
        tree.show(SceneUpdate::from_iter([Change::SetSlot {
            id: SceneNodeId(2),
            slot: SlotChange::Clip(Some(Slot::Value(clip))),
        }]))?;
        let mut direct = Painter::new(&Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)?;
        direct.set_packet_caching(crate::PacketCaching::Disabled);
        assert!(
            paint(&mut cached, &tree)? == paint(&mut direct, &tree)?,
            "{clip:?}"
        );
        assert_eq!(cached.render_stats().cache_hits, hits, "{clip:?}");
    }
    Ok(())
}

#[test]
fn a_card_is_drawn_from_its_image_only_inside_the_surface_shape_corners() -> Result<(), TestError> {
    let outline = skia_safe::RRect::new_rect_xy(skia_safe::Rect::from_wh(120.0, 80.0), 16.0, 16.0);
    let shape = skia_safe::Path::rrect(outline, None);
    for (place, expected) in [(at(0.0, 0.0), 0), (at(28.0, 20.0), 1)] {
        let tree = scene(place, None, card())?;
        let paint = |painter: &mut Painter, canvas: &Canvas| -> Result<(), TestError> {
            let frame = SurfaceFrame {
                tree: tree.at(Duration::ZERO),
                density: Density::ONE,
                damage: Some(Rect::new(0.dp(), 0.dp(), 120.dp(), 80.dp())),
                clear: skia_safe::Color::TRANSPARENT,
                scene: None,
                shape: Some(&shape),
            };
            painter.paint_surface(canvas, frame)?;
            Ok(())
        };
        let hits = cache_hits(ColorType::RGBAF16, |_| Ok(()), paint)?;
        assert_eq!(hits, expected, "{place:?}");
    }
    Ok(())
}

#[test]
fn a_card_blended_in_encoded_srgb_inside_its_layer_paints_directly() -> Result<(), TestError> {
    let tree = scene(at(12.0, 8.0), None, card())?;
    let srgb = |painter: &mut Painter| -> Result<(), TestError> {
        painter.set_blend_space(BlendSpace::Srgb);
        Ok(())
    };
    let hits = cache_hits(ColorType::RGBAF16, srgb, paint_scene(&tree, Density::ONE))?;
    assert_eq!(hits, 0);
    Ok(())
}

#[test]
fn a_dithered_gradient_on_a_card_on_an_8_bit_surface_composites_exactly() -> Result<(), TestError> {
    let gradient = Gradient::linear(
        Point::new(4.dp(), 0.dp()),
        Point::new(60.dp(), 0.dp()),
        &[Color::rgb(0x0010_2040), Color::rgb(0x0018_284a)],
    )?;
    let band = DrawCommand::Fill {
        shape: Rect::new(4.dp(), 24.dp(), 56.dp(), 12.dp()).into(),
        brush: gradient.into(),
    };
    let tree = scene(at(13.0, 9.0), None, [card(), vec![band]].concat())?;
    let hits = cache_hits(
        ColorType::RGBA8888,
        |_| Ok(()),
        paint_scene(&tree, Density::ONE),
    )?;
    assert_eq!(hits, 1);
    Ok(())
}

struct Unseen;

impl EffectBackend for Unseen {
    fn variants(&self, _: &DrawCommand, _: DrawTarget) -> Vec<VariantId> {
        vec![VariantId(1)]
    }

    fn render(&mut self, _: &Canvas, _: &EffectRequest<'_>) {}
}

#[test]
fn a_card_with_a_command_that_an_effect_paints_paints_directly() -> Result<(), TestError> {
    let tree = scene(at(12.0, 8.0), None, card())?;
    let effect = |painter: &mut Painter| -> Result<(), TestError> {
        let circle = |command: &DrawCommand| matches!(command, DrawCommand::Circle { .. });
        painter
            .effect_registry_mut()
            .register_effect(EffectId(911), circle, Unseen)?;
        Ok(())
    };
    let hits = cache_hits(ColorType::RGBAF16, effect, paint_scene(&tree, Density::ONE))?;
    assert_eq!(hits, 0);
    Ok(())
}
