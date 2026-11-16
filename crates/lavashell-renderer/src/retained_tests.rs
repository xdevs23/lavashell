// SPDX-License-Identifier: GPL-3.0-only
use crate::effect_registry::QualityChange;
use crate::test_error::TestError;
use crate::test_scenes::{BACKGROUND_STRIPE_COLORS, TEST_SURFACE, insert, leaves, paint_damage};
use crate::test_surfaces::{StripedSurface, TestSurface, pixels};
use crate::{Fonts, Painter, StreamFrames, SurfaceFrame};
use lavashell_core::prelude::*;
use lavashell_core::retained::UnbalancedRestore;
use lavashell_core::scene::tree::{
    Change, Content, DamageTracker, Node, Packet, SceneNodeId, SceneTree, SceneUpdate,
};
use lavashell_core::surface::SurfaceId;
use lavashell_test_fixtures::WithoutFailures;
use std::sync::Arc;
use std::time::Duration;

const SURFACE: TestSurface = TestSurface::new(240, 120);
const STRIPES: StripedSurface = SURFACE.striped(10);

fn packet(rect: Rect, paint: impl FnOnce(&mut Drawing<'_>)) -> Result<Packet, UnbalancedRestore> {
    let mut commands = Vec::new();
    let context = DrawContext::new(Viewport::new(rect.size(), Density::ONE));
    paint(&mut Drawing::new(&mut commands, rect, &context));
    Packet::new(Arc::from(commands))
}

fn top_leaf(id: u64, index: usize, packet: Packet) -> Change {
    insert(id, None, index, Node::Leaf(Content::Packet(packet)))
}

fn paint(painter: &mut Painter, target: &mut skia_safe::Surface, scene: &SceneSnapshot) {
    painter.paint_scene(
        target.canvas(),
        TEST_SURFACE,
        scene.at(Duration::ZERO),
        Density::ONE,
    );
}

fn repainted_pixels(
    painter: &mut Painter,
    shape: Option<&skia_safe::Path>,
    frames: impl IntoIterator<Item = (SceneSnapshot, Rect)>,
) -> Result<Vec<u64>, TestError> {
    repainted_pixels_on(&mut SURFACE.raster()?, painter, shape, frames)
}

fn repainted_pixels_on(
    target: &mut skia_safe::Surface,
    painter: &mut Painter,
    shape: Option<&skia_safe::Path>,
    frames: impl IntoIterator<Item = (SceneSnapshot, Rect)>,
) -> Result<Vec<u64>, TestError> {
    (frames.into_iter())
        .map(|(tree, damage)| -> Result<u64, TestError> {
            let before = painter.render_stats().damaged_pixels;
            let frame = SurfaceFrame {
                surface: TEST_SURFACE,
                tree: tree.at(Duration::ZERO),
                density: Density::ONE,
                damage: Some(damage),
                clear: skia_safe::Color::TRANSPARENT,
                scene: None,
                shape,
                streams: &StreamFrames::default(),
            };
            painter.paint_surface(target.canvas(), frame)?;
            Ok(painter.render_stats().damaged_pixels - before)
        })
        .collect()
}

fn damage_frames(
    damages: [Rect; 4],
    make_packets: impl Fn() -> Result<Vec<Packet>, UnbalancedRestore>,
) -> Result<Vec<(SceneSnapshot, Rect)>, TestError> {
    let mut frames = Vec::new();
    for damage in damages {
        frames.push((leaves(make_packets()?)?, damage));
    }
    Ok(frames)
}

const DAMAGE_CUTTING_LEAVES: Rect =
    Rect::new(Dp::new(30.0), Dp::new(10.0), Dp::new(80.0), Dp::new(10.0));

fn full_damage_then_cuts() -> [Rect; 4] {
    [
        SURFACE.bounds(),
        DAMAGE_CUTTING_LEAVES,
        DAMAGE_CUTTING_LEAVES,
        DAMAGE_CUTTING_LEAVES,
    ]
}

fn card(rect: Rect) -> Result<Packet, UnbalancedRestore> {
    packet(SURFACE.bounds(), |d| d.fill(rect, Color::rgb(0x002a_4d8f)))
}

fn glass(rect: Rect) -> Result<Packet, UnbalancedRestore> {
    packet(rect, |d| {
        let shape = Shape::rounded(Rect::new(0.dp(), 0.dp(), 70.dp(), 60.dp()), 12.dp());
        let effect = constant!(Backdrop::glass(
            Dp::new(5.0),
            Glass {
                refraction: constant!(NonNegativeDp::try_new(Dp::new(3.0))),
                dispersion: constant!(NonNegativeDp::try_new(Dp::new(0.7))),
                rim: constant!(NonNegative::try_new(0.14)),
            },
        ));
        d.backdrop(shape.clone(), effect);
        d.fill(shape, Color::WHITE.with_alpha(0.14));
    })
}

#[test]
fn partial_damage_matches_full_redraw_as_glass_moves_and_shadow_ends() -> Result<(), TestError> {
    let viewport = SURFACE.bounds();
    let background = packet(viewport, |d| {
        STRIPES.fill_stripes(d, BACKGROUND_STRIPE_COLORS);
    })?;
    let shadow = packet(Rect::new(180.dp(), 30.dp(), 30.dp(), 30.dp()), |d| {
        let shape = Shape::rounded(Rect::new(0.dp(), 0.dp(), 30.dp(), 30.dp()), 8.dp());
        let effect = constant!(Shadow::new(
            Color::rgb(0),
            Dp::new(6.0),
            Point::new(Dp::new(7.0), Dp::new(4.0)),
        ));
        d.shadow(shape.clone(), effect);
        d.fill(shape, Color::WHITE);
    })?;
    let tile = packet(Rect::new(60.dp(), 40.dp(), 20.dp(), 20.dp()), |d| {
        d.fill(
            Rect::new(0.dp(), 0.dp(), 20.dp(), 20.dp()),
            Color::rgb(0x00f4_f0e8),
        );
        let badge = Rect::new(4.dp(), 4.dp(), 12.dp(), 12.dp());
        d.fill(badge, Color::rgb(0x0020_5080).with_alpha(0.7));
    })?;
    let mut retained = SURFACE.raster()?;
    let mut reference = SURFACE.raster()?;
    let mut painter = Painter::new(&Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)?;
    let mut full = Painter::new(&Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)?;
    full.set_packet_caching(crate::PacketCaching::Disabled);
    let (mut tree, mut presented) = (SceneTree::default(), DamageTracker::default());
    for sequence in 0..8_u16 {
        let x = 38.dp() + 9.dp() * f64::from(sequence);
        let moved = glass(Rect::new(x, 20.dp(), 70.dp(), 60.dp()))?;
        let changes = match sequence {
            0 => vec![
                top_leaf(1, 0, background.clone()),
                top_leaf(4, 1, tile.clone()),
                top_leaf(2, 2, moved),
                top_leaf(3, 3, shadow.clone()),
            ],
            4 => vec![
                Change::SetContent {
                    id: SceneNodeId(2),
                    content: Content::Packet(moved),
                },
                Change::Remove { id: SceneNodeId(3) },
            ],
            _ => vec![Change::SetContent {
                id: SceneNodeId(2),
                content: Content::Packet(moved),
            }],
        };
        tree.show(SceneUpdate::from_iter(changes))?;
        let damage = presented.damage(
            tree.at(Duration::ZERO),
            Viewport::new(viewport.size(), Density::ONE),
        );
        let frame = SurfaceFrame {
            surface: TEST_SURFACE,
            tree: tree.at(Duration::ZERO),
            density: Density::ONE,
            damage,
            clear: skia_safe::Color::TRANSPARENT,
            scene: None,
            shape: None,
            streams: &StreamFrames::default(),
        };
        painter.paint_surface(retained.canvas(), frame)?;
        let _canvas_returned_for_chaining = reference.canvas().clear(skia_safe::Color::TRANSPARENT);
        paint(&mut full, &mut reference, &tree.snapshot());
        let a = pixels(&mut retained);
        let b = pixels(&mut reference);
        let worst = a.largest_channel_difference(&b)?;
        assert!(worst <= 2, "frame {sequence}, worst channel delta {worst}");
    }
    assert!(painter.render_stats().cache_hits > 0);
    assert!(painter.render_stats().damaged_pixels < 8 * SURFACE.pixel_count());
    Ok(())
}

#[test]
fn a_damage_rectangle_repaints_every_pixel_that_it_covers_in_part() -> Result<(), TestError> {
    let viewport = SURFACE.bounds();
    let mut painter = Painter::new(&Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)?;
    let mut target = SURFACE.raster()?;
    let damaged = Rect::new(4.6.dp(), 2.6.dp(), 5.dp(), 5.dp());
    let disc = |color| DrawCommand::Circle {
        center: Point::new(120.dp(), 60.dp()),
        radius: 140.dp(),
        brush: Color::rgb(color).into(),
    };
    for (color, damage) in [(0x00ff_0000, viewport), (0x0000_00ff, damaged)] {
        let tree = leaves([Packet::new(Arc::from([disc(color)]))?])?;
        let clear = skia_safe::Color::TRANSPARENT;
        paint_damage(
            &mut painter,
            TEST_SURFACE,
            target.canvas(),
            &tree,
            Some(damage),
            clear,
        )?;
    }
    let shown = pixels(&mut target);
    let pixel = |x, y| shown.at(x, y);
    for (x, y) in [(4, 2), (9, 7)] {
        assert_eq!(pixel(x, y), [0, 0, 255, 255], "({x}, {y})");
    }
    for (x, y) in [(3, 2), (10, 7), (9, 8)] {
        assert_eq!(pixel(x, y), [255, 0, 0, 255], "({x}, {y})");
    }
    Ok(())
}

#[test]
fn a_stable_leaf_cut_by_damage_is_copied_once_then_drawn_from_the_copy() -> Result<(), TestError> {
    let stable = card(Rect::new(0.dp(), 0.dp(), 40.dp(), 40.dp()))?;
    let mut painter = Painter::new(&Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)?;
    let frames = damage_frames(full_damage_then_cuts(), || Ok(vec![stable.clone()]))?;
    let shown = repainted_pixels(&mut painter, None, frames)?;
    let (cut, stable_and_cut) = (80 * 10, 110 * 41);
    assert_eq!(shown, [SURFACE.pixel_count(), stable_and_cut, cut, cut]);
    assert_eq!(painter.render_stats().cache_misses, 1);
    assert_eq!(painter.render_stats().cache_hits, 2);
    Ok(())
}

#[test]
fn a_leaf_with_a_changing_packet_neither_fills_nor_widens_the_damage() -> Result<(), TestError> {
    let stable = card(Rect::new(0.dp(), 0.dp(), 40.dp(), 40.dp()))?;
    let transient = || card(Rect::new(100.dp(), 0.dp(), 40.dp(), 40.dp()));
    let mut painter = Painter::new(&Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)?;
    let frames = damage_frames(full_damage_then_cuts(), || {
        Ok(vec![stable.clone(), transient()?])
    })?;
    let shown = repainted_pixels(&mut painter, None, frames)?;
    let (cut, stable_and_cut) = (80 * 10, 110 * 41);
    assert_eq!(shown, [SURFACE.pixel_count(), stable_and_cut, cut, cut]);
    assert_eq!(painter.render_stats().cache_misses, 1);
    Ok(())
}

#[test]
fn a_stable_leaf_without_cache_room_neither_fills_nor_widens_the_damage() -> Result<(), TestError> {
    let first = card(Rect::new(0.dp(), 0.dp(), 40.dp(), 40.dp()))?;
    let second = card(Rect::new(100.dp(), 0.dp(), 40.dp(), 40.dp()))?;
    let mut gpu = crate::VulkanRenderer::headless()?;
    let mut painter = gpu.painter(&Fonts::new())?;
    let frames = damage_frames(full_damage_then_cuts(), || {
        Ok(vec![first.clone(), second.clone()])
    })?;
    let mut target = gpu.offscreen(SURFACE.pixel_size())?;
    let one_copy = 40 * 40 * target.image_info().bytes_per_pixel();
    target
        .canvas()
        .direct_context()
        .ok_or(TestError::Missing("GPU context of the canvas"))?
        .set_resource_cache_limit(one_copy);
    let shown = repainted_pixels_on(&mut target, &mut painter, None, frames)?;
    let (cut, first_and_cut) = (80 * 10, 110 * 41);
    assert_eq!(shown, [SURFACE.pixel_count(), first_and_cut, cut, cut]);
    let stats = painter.render_stats();
    assert_eq!((stats.cache_misses, stats.cache_hits), (1, 2));
    assert_eq!(stats.uncached_leaves, 2 + 3);
    Ok(())
}

#[test]
fn damage_covering_only_the_edge_of_a_stable_leaf_still_copies_the_leaf() -> Result<(), TestError> {
    let stable = card(Rect::new(0.dp(), 0.dp(), 40.dp(), 40.dp()))?;
    let edge = Rect::new(40.dp(), 10.dp(), 10.dp(), 10.dp());
    let mut painter = Painter::new(&Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)?;
    let frames = damage_frames([SURFACE.bounds(), edge, edge, edge], || {
        Ok(vec![stable.clone()])
    })?;
    let shown = repainted_pixels(&mut painter, None, frames)?;
    let (edge, leaf_and_edge) = (10 * 10, 50 * 41);
    assert_eq!(shown, [SURFACE.pixel_count(), leaf_and_edge, edge, edge]);
    let stats = painter.render_stats();
    assert_eq!((stats.cache_misses, stats.cache_hits), (1, 2));
    Ok(())
}

#[test]
fn a_stable_leaf_cut_by_window_shape_neither_fills_nor_widens_damage() -> Result<(), TestError> {
    let stable = card(Rect::new(0.dp(), 0.dp(), 40.dp(), 40.dp()))?;
    let shape = skia_safe::Path::rect(skia_safe::Rect::from_wh(20.0, 120.0), None);
    let damage = Rect::new(10.dp(), 10.dp(), 20.dp(), 10.dp());
    let mut painter = Painter::new(&Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)?;
    let frames = damage_frames([SURFACE.bounds(), damage, damage, damage], || {
        Ok(vec![stable.clone()])
    })?;
    let shown = repainted_pixels(&mut painter, Some(&shape), frames)?;
    assert_eq!(shown, [SURFACE.pixel_count(), 20 * 10, 20 * 10, 20 * 10]);
    assert_eq!(painter.render_stats().cache_misses, 0);
    Ok(())
}

#[test]
fn the_same_leaf_in_different_windows_reuses_the_cached_images() -> Result<(), TestError> {
    let rect = Rect::new(0.dp(), 0.dp(), 100.dp(), 100.dp());
    let red = leaves([packet(rect, |d| d.fill(rect, Color::rgb(0x00ff_0000)))?])?;
    let blue = leaves([packet(rect, |d| d.fill(rect, Color::rgb(0x0000_00ff)))?])?;
    let mut painter = Painter::new(&Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)?;
    let mut target = SURFACE.raster()?;
    for (frame, window, tree) in [
        (1, 11, &red),
        (1, 21, &blue),
        (2, 11, &red),
        (2, 21, &blue),
        (3, 11, &red),
        (3, 21, &blue),
    ] {
        assert_eq!(
            painter.plan_scene_effects(
                tree.at(Duration::ZERO),
                lavashell_effect_quality::FrameBudget {
                    surface: SurfaceId::numbered(window),
                    gpu_time: Some(Duration::ZERO),
                },
                frame,
                0,
                Density::ONE,
            ),
            QualityChange::Unchanged
        );
        let shown = tree.at(Duration::ZERO);
        painter.paint_scene(
            target.canvas(),
            SurfaceId::numbered(window),
            shown,
            Density::ONE,
        );
    }
    assert_eq!(painter.render_stats().cache_misses, 2);
    assert_eq!(painter.render_stats().cache_hits, 2);
    painter.forget_surface(SurfaceId::numbered(11));
    assert!(painter.render_stats().cache_bytes > 0);
    painter.forget_surface(SurfaceId::numbered(21));
    assert_eq!(painter.render_stats().cache_bytes, 0);
    Ok(())
}

#[test]
fn transient_packets_paint_correctly_without_allocating_cached_images() -> Result<(), TestError> {
    let rect = Rect::new(0.dp(), 0.dp(), 100.dp(), 100.dp());
    let mut painter = Painter::new(&Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)?;
    let mut reference = Painter::new(&Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)?;
    reference.set_packet_caching(crate::PacketCaching::Disabled);
    let mut target = SURFACE.raster()?;
    let mut full = SURFACE.raster()?;
    let mut tree = SceneTree::default();
    for generation in 1..=8 {
        let filled = packet(rect, |d| d.fill(rect, Color::rgb(0x0012_3456)))?;
        let change = if generation == 1 {
            top_leaf(1, 0, filled)
        } else {
            Change::SetContent {
                id: SceneNodeId(1),
                content: Content::Packet(filled),
            }
        };
        tree.show(SceneUpdate::from_iter([change]))?;
        let scene = tree.snapshot();
        paint(&mut painter, &mut target, &scene);
        paint(&mut reference, &mut full, &scene);
        assert_eq!(pixels(&mut target), pixels(&mut full));
    }
    assert_eq!(painter.render_stats().cache_misses, 0);
    assert_eq!(painter.render_stats().cache_bytes, 0);
    assert_eq!(painter.render_stats().uncached_leaves, 8);
    Ok(())
}

#[test]
fn a_new_view_painted_by_the_same_painter_shows_its_own_commands() -> Result<(), TestError> {
    let viewport = Viewport::new(SURFACE.size(), Density::ONE);
    let mut painter = Painter::new(&Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)?;
    let mut target = SURFACE.raster()?;
    for color in [Color::rgb(0x00ff_0000), Color::rgb(0x0000_00ff)] {
        let mut view = View::new(lavashell_test_fixtures::facts(), move |ui| {
            ui.canvas(Size::new(100.dp(), 100.dp()), Modifier::new(), move |d| {
                d.fill(Rect::new(0.dp(), 0.dp(), 100.dp(), 100.dp()), color);
            });
        });
        let mut builder = SceneBuilder::default();
        (view.frame_into(viewport, &mut painter.shaper, &mut builder)).without_failures()?;
        let scene = builder.snapshot();
        for _ in 0..3 {
            paint(&mut painter, &mut target, &scene);
        }
    }
    assert!(painter.render_stats().cache_hits > 0);
    assert_eq!(pixels(&mut target).at(50, 60), [0, 0, 255, 255]);
    Ok(())
}
