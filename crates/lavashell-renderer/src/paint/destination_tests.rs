// SPDX-License-Identifier: GPL-3.0-only
use crate::effect_registry::{DrawTarget, EffectBackend, EffectRequest};
use crate::failure::DrawFailure;
use crate::scalar::scalar;
use crate::test_error::TestError;
use crate::test_scenes::insert;
use crate::test_surfaces::{self, ChannelRounding};
use crate::{Fonts, Painter, SurfaceFrame};
use lavashell_core::prelude::*;
use lavashell_core::retained::Extent;
use lavashell_core::scene::tree::{
    Change, Content, DamageTracker, Group, Node, Packet, SceneNodeId, SceneTree, SceneUpdate, Slot,
};
use lavashell_core::surface::SurfaceId;
use lavashell_effect_quality::{EffectId, VariantId};
use skia_safe::{AlphaType, Canvas, ColorType, ImageInfo, Paint, RRect, Surface};
use skia_safe::{canvas::SaveLayerRec, image_filters};
use std::sync::Arc;
use std::time::Duration;

struct Reader;

impl EffectBackend for Reader {
    fn variants(&self, _: &DrawCommand, _: DrawTarget) -> Vec<VariantId> {
        vec![VariantId(1)]
    }

    fn reads_destination(&self) -> bool {
        true
    }

    fn render(&mut self, canvas: &Canvas, request: &EffectRequest<'_>) {
        if let Err(failure) = blur_backdrop(canvas, request.command) {
            request.painter.record(failure);
        }
    }
}

fn blur_backdrop(canvas: &Canvas, command: &DrawCommand) -> Result<(), DrawFailure> {
    let DrawCommand::Circle { center, radius, .. } = command else {
        return Ok(());
    };
    let bounds = crate::paint::skrect(Rect::new(
        center.x - *radius,
        center.y - *radius,
        *radius * 2.0,
        *radius * 2.0,
    ))?;
    let radius = scalar(radius.get())?;
    let filter = image_filters::blur((5.0, 5.0), skia_safe::TileMode::Clamp, None, None)
        .ok_or(DrawFailure::ShaderRejected)?;
    let saved = canvas.save();
    let _chained_canvas = canvas.clip_rrect(RRect::new_rect_xy(bounds, radius, radius), None, true);
    let mut paint = Paint::default();
    let _chained_paint = paint.set_blend_mode(skia_safe::BlendMode::Src);
    let _save_count = canvas.save_layer(
        &SaveLayerRec::default()
            .bounds(&bounds)
            .paint(&paint)
            .backdrop(&filter),
    );
    let _chained_canvas = canvas.restore();
    let _chained_canvas = canvas.restore_to_count(saved);
    Ok(())
}

fn new_painter() -> Result<Painter, TestError> {
    let mut painter = Painter::new(&Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)?;
    let id = EffectId(0x1234);
    painter.effect_registry_mut().register_effect(
        id,
        |c| matches!(c, DrawCommand::Circle { .. }),
        Reader,
    )?;
    Ok(painter)
}

fn surface() -> Result<Surface, TestError> {
    test_surfaces::surface(96, 48)
}

fn tile(color: Color) -> Vec<DrawCommand> {
    tile_at(color, 32.dp())
}

fn tile_at(color: Color, x: Dp) -> Vec<DrawCommand> {
    vec![DrawCommand::Fill {
        shape: Rect::new(x, 0.dp(), 4.dp(), 48.dp()).into(),
        brush: color.into(),
    }]
}

fn reader_bounds(x: Dp) -> Rect {
    Rect::new(x, 16.dp(), 16.dp(), 16.dp())
}

fn reader(include_known_reader: bool) -> Vec<DrawCommand> {
    reader_at(include_known_reader, 40.dp())
}

fn reader_at(include_known_reader: bool, x: Dp) -> Vec<DrawCommand> {
    let bounds = reader_bounds(x);
    let mut commands = vec![
        DrawCommand::OpacityLayer { alpha: 0.5, bounds },
        DrawCommand::Circle {
            center: Point::new(x + 8.dp(), 24.dp()),
            radius: 8.dp(),
            brush: Color::WHITE.into(),
        },
        DrawCommand::Restore,
    ];
    if include_known_reader {
        commands.push(DrawCommand::Backdrop {
            shape: Rect::new(x + 44.dp(), 20.dp(), 8.dp(), 8.dp()).into(),
            effect: constant!(Backdrop::blur(Dp::new(2.0))),
        });
    }
    commands
}

fn packet(commands: Vec<DrawCommand>) -> Packet {
    Packet::new(Arc::from(commands))
}

fn leaf(packet: Packet) -> Node {
    Node::Leaf(Content::Packet(packet))
}

fn set(id: u64, packet: Packet) -> Change {
    Change::SetContent {
        id: SceneNodeId(id),
        content: Content::Packet(packet),
    }
}

fn paint_damage(
    painter: &mut Painter,
    target: &mut Surface,
    tree: &SceneTree,
    density: Density,
    damage: Option<Rect>,
) -> Result<(), TestError> {
    let frame = SurfaceFrame {
        tree: tree.at(Duration::ZERO),
        density,
        damage,
        clear: skia_safe::Color::BLACK,
        scene: None,
        shape: None,
    };
    painter.paint_surface(target.canvas(), frame)?;
    Ok(())
}

fn paint_full(painter: &mut Painter, target: &mut Surface, tree: &SceneTree, density: Density) {
    let _chained_canvas = target.canvas().clear(skia_safe::Color::BLACK);
    crate::test_scenes::paint(painter, target.canvas(), tree, density);
}

/// How far a channel of a partial redraw may lie from the same channel `full` of a full redraw on `surface`. Each
/// redraw rounds a pixel up to three times: the tile, the blurred copy of the reader and its opacity layer.
fn redraw_rounding(surface: &Surface, full: f64) -> Result<f64, TestError> {
    Ok(ChannelRounding::of(surface)?.after(2 * 3, full))
}

fn red(surface: &mut Surface, x: i32, y: i32) -> f64 {
    let info = ImageInfo::new(
        (1, 1),
        ColorType::RGBAF32,
        AlphaType::Premul,
        skia_safe::ColorSpace::new_srgb_linear(),
    );
    let mut bytes = [0u8; 16];
    assert!(surface.read_pixels(&info, &mut bytes, 16, (x, y)));
    let (floats, _) = bytes.as_chunks::<4>();
    f64::from(f32::from_ne_bytes(floats[0]))
}

#[test]
fn partial_damage_resamples_custom_reader_halo_outside_its_output() -> Result<(), TestError> {
    assert_partial_damage(false, false)?;
    assert_partial_damage(true, false)
}

#[test]
fn partial_damage_reconstructs_unknown_sampling_inside_isolated_layers() -> Result<(), TestError> {
    assert_partial_damage(false, true)
}

#[test]
fn isolated_packet_sampling_does_not_require_exterior_parent() -> Result<(), TestError> {
    let viewport = Rect::new(0.dp(), 0.dp(), 96.dp(), 48.dp());
    let mut painter = new_painter()?;
    let packet = Packet::new(isolate(reader(false), viewport).into());
    let analysis = (painter.analyses).get(SurfaceId(0), &packet, &painter.effect_registry);
    assert!(!analysis.reads_parent());
    assert!(analysis.unknown_sampling());
    Ok(())
}

#[test]
fn transformed_packets_match_full_paint_and_damage_every_visible_change() -> Result<(), TestError> {
    for in_packet in [false, true] {
        for scale in [1.0, 1.5] {
            assert_transformed_damage(in_packet, scale)?;
        }
    }
    Ok(())
}

fn translated(generation: u64, color: Color, in_packet: bool) -> Vec<Change> {
    let moved = Affine::translate(Point::new(-96.dp(), 0.dp()));
    if in_packet {
        let commands = [
            vec![DrawCommand::Save, DrawCommand::Transform(moved)],
            tile_at(color, 128.dp()),
            reader_at(false, 136.dp()),
            vec![DrawCommand::Restore],
        ]
        .concat();
        return match generation {
            1 => vec![insert(1, None, 0, leaf(packet(commands)))],
            _ => vec![set(1, packet(commands))],
        };
    }
    let tile = packet(tile_at(color, 128.dp()));
    if generation > 1 {
        return vec![set(2, tile)];
    }
    let group = Group {
        transform: Slot::Value(moved),
        ..Group::default()
    };
    let reader = packet(reader_at(false, 136.dp()));
    vec![
        insert(1, None, 0, Node::Group(group)),
        insert(2, Some(1), 0, leaf(tile)),
        insert(3, Some(1), 1, leaf(reader)),
    ]
}

fn assert_transformed_damage(in_packet: bool, scale: f64) -> Result<(), TestError> {
    let density = Density::from_scale(scale).ok_or(TestError::NoDensity(scale))?;
    let viewport = Viewport::new(Size::new(96.dp() / scale, 48.dp() / scale), density);
    let (mut tree, mut presented) = (SceneTree::default(), DamageTracker::default());
    let mut partial = new_painter()?;
    let mut full = new_painter()?;
    let mut retained = surface()?;
    let mut reference = surface()?;
    for (generation, color) in [(1, Color::WHITE), (2, Color::rgb(0))] {
        tree.show(SceneUpdate::from_iter(translated(
            generation, color, in_packet,
        )))?;
        let dirty = presented.damage(tree.at(Duration::ZERO), viewport);
        let changed = Point::new(34.dp(), 24.dp());
        assert!(
            dirty.is_some_and(|dirty| dirty.contains(changed)),
            "visible transformed changes need damage"
        );
        paint_damage(&mut partial, &mut retained, &tree, density, dirty)?;
        paint_full(&mut full, &mut reference, &tree, density);
        let (x, y) = (scalar(42.0 * scale)?, scalar(24.0 * scale)?);
        let sampled = skia_safe::Rect::from_xywh(x, y, 1.0, 1.0).round();
        let partial_red = red(&mut retained, sampled.left, sampled.top);
        let full_red = red(&mut reference, sampled.left, sampled.top);
        if generation == 1 {
            assert!(
                full_red > 0.01,
                "reference must sample the visible white stripe"
            );
        } else {
            assert!(full_red < 0.003, "changed stripe must replace the old blur");
        }
        assert!(
            (partial_red - full_red).abs() <= redraw_rounding(&reference, full_red)?,
            "visible transformed packet was skipped"
        );
        assert_eq!(
            presented.damage(tree.at(Duration::ZERO), viewport),
            None,
            "unchanged frame remains idle"
        );
    }
    Ok(())
}

fn isolate(mut commands: Vec<DrawCommand>, viewport: Rect) -> Vec<DrawCommand> {
    commands.insert(
        0,
        DrawCommand::EffectLayer {
            bounds: viewport,
            blend: BlendMode::SourceOver,
            blur: NonNegativeDp::ZERO,
            filter: None,
        },
    );
    commands.push(DrawCommand::Restore);
    commands
}

fn assert_partial_damage(include_known_reader: bool, isolated: bool) -> Result<(), TestError> {
    let viewport = Viewport::new(Size::new(96.dp(), 48.dp()), Density::ONE);
    let reader = packet(reader(include_known_reader));
    if !include_known_reader {
        assert_eq!(
            reader.footprint().output,
            Some(Extent::Within(reader_bounds(40.dp())))
        );
        assert_eq!(reader.footprint().backdrop, None);
    }
    let area = Rect::from_size(viewport.size);
    let both = |color| isolate([tile(color), reader.commands().to_vec()].concat(), area);
    let (mut tree, mut presented) = (SceneTree::default(), DamageTracker::default());
    let (first, changed) = if isolated {
        let first = vec![insert(1, None, 0, leaf(packet(both(Color::rgb(0)))))];
        (first, set(1, packet(both(Color::WHITE))))
    } else {
        let first = vec![
            insert(1, None, 0, leaf(packet(tile(Color::rgb(0))))),
            insert(2, None, 1, leaf(reader.clone())),
        ];
        (first, set(1, packet(tile(Color::WHITE))))
    };
    tree.show(SceneUpdate::from_iter(first))?;
    let mut partial = new_painter()?;
    let mut full = new_painter()?;
    let mut retained = surface()?;
    let mut reference = surface()?;
    let first = presented.damage(tree.at(Duration::ZERO), viewport);
    paint_damage(&mut partial, &mut retained, &tree, Density::ONE, first)?;
    tree.show(SceneUpdate::from_iter(vec![changed]))?;
    let dirty = presented.damage(tree.at(Duration::ZERO), viewport);
    assert!(dirty.is_some(), "the change must damage the frame");
    if !isolated {
        assert_eq!(dirty, Some(Rect::new(31.dp(), 0.dp(), 6.dp(), 48.dp())));
    }
    paint_damage(&mut partial, &mut retained, &tree, Density::ONE, dirty)?;
    paint_full(&mut full, &mut reference, &tree, Density::ONE);
    let partial_red = red(&mut retained, 42, 24);
    let full_red = red(&mut reference, 42, 24);
    assert!(full_red > 0.01, "reference must sample the changed stripe");
    assert!(
        (partial_red - full_red).abs() <= redraw_rounding(&reference, full_red)?,
        "partial redraw left stale reader output"
    );
    let painted = partial.render_stats().damaged_pixels;
    let unchanged = presented.damage(tree.at(Duration::ZERO), viewport);
    assert_eq!(unchanged, None);
    paint_damage(&mut partial, &mut retained, &tree, Density::ONE, unchanged)?;
    assert_eq!(
        partial.render_stats().damaged_pixels,
        painted,
        "unchanged frames must remain idle"
    );
    Ok(())
}
