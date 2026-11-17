// SPDX-License-Identifier: GPL-3.0-only
use crate::destination::Destination;
use crate::plan::Area;
use crate::test_error::TestError;
use crate::test_font::probe_style;
use crate::test_scenes::{BACKGROUND_STRIPE_COLORS, TEST_SURFACE, insert};
use crate::test_surfaces::{Pixels, StripedSurface, TestSurface, pixels};
use crate::{Fonts, Painter, StreamFrames, SurfaceFrame};
use lavashell_core::prelude::*;
use lavashell_core::retained::UnbalancedRestore;
use lavashell_core::scene::tree::*;
use std::{sync::Arc, time::Duration};

pub(super) const SURFACE: TestSurface = TestSurface::new(160, 120);
pub(super) const STRIPES: StripedSurface = SURFACE.striped(10);

pub(super) fn shapes() -> Vec<DrawCommand> {
    vec![
        DrawCommand::Fill {
            shape: Rect::new(0.dp(), 0.dp(), 40.dp(), 30.dp()).into(),
            brush: Color::rgb(0x00d0_4020).into(),
        },
        DrawCommand::Circle {
            center: Point::new(36.dp(), 26.dp()),
            radius: 14.dp(),
            brush: Color::rgb(0x0020_50d0).with_alpha(0.8).into(),
        },
    ]
}

fn stripes() -> Vec<DrawCommand> {
    STRIPES.stripe_fills(BACKGROUND_STRIPE_COLORS)
}

fn glass() -> Vec<DrawCommand> {
    let shape = Shape::rounded(Rect::new(0.dp(), 0.dp(), 60.dp(), 50.dp()), 12.dp());
    vec![
        DrawCommand::Backdrop {
            shape: shape.clone(),
            effect: constant!(Backdrop::glass(
                Dp::new(4.0),
                Glass {
                    refraction: constant!(NonNegativeDp::try_new(Dp::new(12.0))),
                    dispersion: constant!(NonNegativeDp::try_new(Dp::new(0.7))),
                    rim: constant!(NonNegative::try_new(0.14)),
                },
            )),
        },
        DrawCommand::Fill {
            shape,
            brush: Color::WHITE.with_alpha(0.2).into(),
        },
    ]
}

pub(super) fn leaf(commands: Vec<DrawCommand>) -> Result<Node, UnbalancedRestore> {
    Ok(Node::Leaf(Content::Packet(Packet::new(Arc::from(
        commands,
    ))?)))
}

fn group(transform: Affine, opacity: f64, clip: Option<RoundedRect>) -> Node {
    Node::Group(Group {
        transform: Slot::Value(transform),
        opacity: Slot::Value(opacity),
        clip: clip.map(Slot::Value),
    })
}

fn tree(changes: impl IntoIterator<Item = Change>) -> Result<SceneTree, TestError> {
    let mut tree = SceneTree::default();
    tree.show(SceneUpdate::from_iter(changes))?;
    Ok(tree)
}

fn grouped(node: Node, commands: Vec<DrawCommand>) -> Result<SceneTree, TestError> {
    tree([
        insert(1, None, 0, node),
        insert(2, Some(1), 0, leaf(commands)?),
    ])
}

fn painted(tree: &SceneTree, density: Density) -> Result<Pixels<[u8; 4]>, TestError> {
    let mut painter = Painter::new(&Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)?;
    painter.set_packet_caching(crate::PacketCaching::Disabled);
    let mut target = SURFACE.raster()?;
    painter.paint_scene(
        target.canvas(),
        TEST_SURFACE,
        tree.at(Duration::ZERO),
        density,
    )?;
    Ok(pixels(&mut target))
}

#[test]
fn a_leaf_under_a_group_transform_paints_like_its_commands_moved() -> Result<(), TestError> {
    let offset = Point::new(17.25.dp(), 9.5.dp());
    for transform in [
        Affine::translate(offset),
        Affine::scale(1.5, 0.75).translated(offset),
        Affine::rotate(0.4).translated(offset),
    ] {
        let moved = [vec![DrawCommand::Transform(transform)], shapes()].concat();
        assert_eq!(
            painted(
                &grouped(group(transform, 1.0, None), shapes())?,
                Density::ONE
            )?,
            painted(
                &grouped(Node::Group(Group::default()), moved)?,
                Density::ONE
            )?,
            "{transform:?}"
        );
    }
    Ok(())
}

#[test]
fn a_leaf_under_a_faded_group_paints_like_its_commands_faded_together() -> Result<(), TestError> {
    let faded = [
        vec![DrawCommand::Opacity(0.5)],
        shapes(),
        vec![DrawCommand::Restore],
    ]
    .concat();
    let identity = Affine::IDENTITY;
    assert_eq!(
        painted(
            &grouped(group(identity, 0.5, None), shapes())?,
            Density::ONE
        )?,
        painted(
            &grouped(Node::Group(Group::default()), faded)?,
            Density::ONE
        )?
    );
    Ok(())
}

#[test]
fn a_leaf_under_a_clipping_group_paints_like_its_commands_clipped() -> Result<(), TestError> {
    let rect = Rect::new(6.dp(), 4.dp(), 34.dp(), 28.dp());
    let clip = DrawCommand::Clip {
        rect,
        corners: Corners::all(9.dp()).into(),
    };
    let clipped = [
        vec![DrawCommand::Save, clip],
        shapes(),
        vec![DrawCommand::Restore],
    ]
    .concat();
    let clip = RoundedRect::new(rect, Corners::all(9.dp()));
    let scale = Density::from_scale(1.5).ok_or(TestError::NoDensity(1.5))?;
    assert_eq!(
        painted(
            &grouped(group(Affine::IDENTITY, 1.0, Some(clip)), shapes())?,
            scale
        )?,
        painted(&grouped(Node::Group(Group::default()), clipped)?, scale)?
    );
    Ok(())
}

#[test]
fn nested_groups_compose_their_transforms_clips_and_opacities() -> Result<(), TestError> {
    let outer = Affine::translate(Point::new(20.5.dp(), 12.dp()));
    let inner = Affine::scale(1.25, 1.25).translated(Point::new(30.dp(), 18.dp()));
    let clip = RoundedRect::new(
        Rect::new(4.dp(), 4.dp(), 30.dp(), 26.dp()),
        Corners::all(6.dp()),
    );
    let nested = tree([
        insert(1, None, 0, group(outer, 0.8, None)),
        insert(2, Some(1), 0, leaf(shapes())?),
        insert(3, Some(1), 1, group(inner, 0.5, Some(clip))),
        insert(4, Some(3), 0, leaf(shapes())?),
    ])?;
    let flat = [
        vec![
            DrawCommand::Save,
            DrawCommand::Transform(outer),
            DrawCommand::Opacity(0.8),
        ],
        shapes(),
        vec![
            DrawCommand::Save,
            DrawCommand::Transform(inner),
            DrawCommand::Clip {
                rect: clip.rect,
                corners: clip.corners.into(),
            },
            DrawCommand::Opacity(0.5),
        ],
        shapes(),
        vec![DrawCommand::Restore; 4],
    ]
    .concat();
    assert_eq!(
        painted(&nested, Density::ONE)?,
        painted(&grouped(Node::Group(Group::default()), flat)?, Density::ONE)?
    );
    Ok(())
}

#[test]
fn a_faded_group_around_glass_starts_from_the_pixels_below_it() -> Result<(), TestError> {
    let place = Affine::translate(Point::new(40.dp(), 30.dp()));
    let scene = |opacity, commands| {
        tree([
            insert(1, None, 0, leaf(stripes())?),
            insert(2, None, 1, group(place, opacity, None)),
            insert(3, Some(2), 0, leaf(commands)?),
        ])
    };
    let faded = [
        vec![DrawCommand::Opacity(0.5)],
        glass(),
        vec![DrawCommand::Restore],
    ]
    .concat();
    assert_eq!(
        painted(&scene(0.5, glass())?, Density::ONE)?,
        painted(&scene(1.0, faded)?, Density::ONE)?
    );
    Ok(())
}

#[test]
fn a_graft_paints_the_namespace_that_it_shows_in_place() -> Result<(), TestError> {
    let place = Affine::rotate(0.2).translated(Point::new(30.dp(), 20.dp()));
    let mut grafted = SceneTree::default();
    let guest = grafted.create_namespace();
    grafted.apply(
        guest,
        SceneUpdate::from_iter([insert(1, None, 0, leaf(shapes())?)]),
    )?;
    let graft = Node::Leaf(Content::Graft(guest));
    grafted.apply(
        SceneTree::ROOT,
        SceneUpdate::from_iter([
            insert(1, None, 0, group(place, 0.7, None)),
            insert(2, Some(1), 0, graft),
        ]),
    )?;
    assert_eq!(
        painted(&grafted, Density::ONE)?,
        painted(&grouped(group(place, 0.7, None), shapes())?, Density::ONE)?
    );
    Ok(())
}

#[test]
fn a_layer_group_without_an_extent_paints_nothing() -> Result<(), TestError> {
    let nothing = vec![DrawCommand::Save, DrawCommand::Restore];
    let scene = grouped(group(Affine::IDENTITY, 0.5, None), nothing)?;
    let mut painter = Painter::new(&Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)?;
    let mut target = SURFACE.raster()?;
    let _canvas_returned_for_chaining = target.canvas().clear(skia_safe::Color::WHITE);
    let before = pixels(&mut target);
    let plan = painter.plan(
        target.canvas(),
        TEST_SURFACE,
        scene.at(Duration::ZERO),
        None,
    );
    let [crate::plan::Planned::Group { group: planned, .. }] = plan.steps.as_slice() else {
        return Err(TestError::Missing("group"));
    };
    assert_eq!(planned.opacity.destination(), Destination::Layer);
    assert_eq!(planned.device_extent, None);
    let streams = StreamFrames::default();
    painter.paint_tree(target.canvas(), &plan, Area::Everything, &streams);
    assert_eq!(pixels(&mut target), before);
    Ok(())
}

fn card() -> Vec<DrawCommand> {
    let card = DrawCommand::Fill {
        shape: Rect::new((-2).dp(), (-2).dp(), 56.dp(), 46.dp()).into(),
        brush: Color::rgb(0x00f4_f0e8).into(),
    };
    [vec![card], shapes()].concat()
}

#[test]
fn a_group_reuses_the_cached_image_of_its_leaf_only_where_the_image_was_copied()
-> Result<(), TestError> {
    let at = |x: f64, y: f64| Affine::translate(Point::new(x.dp(), y.dp()));
    let mut scene = grouped(group(at(10.0, 6.0), 1.0, None), card())?;
    let mut painter = Painter::new(&Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)?;
    let mut target = SURFACE.raster()?;
    let mut paint = |painter: &mut Painter, scene: &SceneTree| {
        let _canvas_returned_for_chaining = target.canvas().clear(skia_safe::Color::TRANSPARENT);
        painter.paint_scene(
            target.canvas(),
            TEST_SURFACE,
            scene.at(Duration::ZERO),
            Density::ONE,
        )?;
        Ok::<_, TestError>(pixels(&mut target))
    };
    let direct = painted(&scene, Density::ONE)?;
    assert_eq!(paint(&mut painter, &scene)?, direct);
    assert_eq!(paint(&mut painter, &scene)?, direct);
    assert_eq!(painter.render_stats().cache_misses, 1);
    for (x, y, hits) in [
        (10.0, 6.0, 1),
        (41.0, 20.0, 1),
        (10.0, 6.0, 2),
        (10.5, 6.0, 2),
    ] {
        let moved = Change::SetSlot {
            id: SceneNodeId(1),
            slot: SlotChange::Transform(Slot::Value(at(x, y))),
        };
        scene.show(SceneUpdate::from_iter([moved]))?;
        assert_eq!(
            paint(&mut painter, &scene)?,
            painted(&scene, Density::ONE)?,
            "{x}, {y}"
        );
        assert_eq!(painter.render_stats().cache_hits, hits, "{x}, {y}");
    }
    Ok(())
}

#[test]
fn partial_damage_from_the_tree_matches_a_full_paint_while_glass_moves() -> Result<(), TestError> {
    let viewport = Viewport::new(Size::new(160.dp(), 120.dp()), Density::ONE);
    let at = |sequence: f64| Affine::translate(Point::new(20.dp() + 9.dp() * sequence, 30.dp()));
    let below = Affine::translate(Point::new(40.dp(), 40.dp()));
    let tile = vec![
        DrawCommand::Fill {
            shape: Rect::new(0.dp(), 0.dp(), 20.dp(), 20.dp()).into(),
            brush: Color::rgb(0x00f4_f0e8).into(),
        },
        DrawCommand::Fill {
            shape: Rect::new(4.dp(), 4.dp(), 12.dp(), 12.dp()).into(),
            brush: Color::rgb(0x0020_50d0).with_alpha(0.8).into(),
        },
    ];
    let mut scene = tree([
        insert(1, None, 0, leaf(stripes())?),
        insert(4, None, 1, group(below, 1.0, None)),
        insert(5, Some(4), 0, leaf(tile)?),
        insert(2, None, 2, group(at(0.0), 1.0, None)),
        insert(3, Some(2), 0, leaf(glass())?),
    ])?;
    let mut painter = Painter::new(&Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)?;
    let mut retained = SURFACE.raster()?;
    let mut presented = DamageTracker::default();
    for sequence in 0..8_u8 {
        let moved = Change::SetSlot {
            id: SceneNodeId(2),
            slot: SlotChange::Transform(Slot::Value(at(f64::from(sequence)))),
        };
        scene.show(SceneUpdate::from_iter([moved]))?;
        let damage = presented.damage(scene.at(Duration::ZERO), viewport);
        painter.paint_surface(
            retained.canvas(),
            SurfaceFrame {
                surface: TEST_SURFACE,
                tree: scene.at(Duration::ZERO),
                dp_to_output: Affine::IDENTITY,
                damage,
                clear: skia_safe::Color::TRANSPARENT,
                scene: None,
                shape: None,
                streams: &StreamFrames::default(),
            },
        )?;
        assert_eq!(
            pixels(&mut retained),
            painted(&scene, Density::ONE)?,
            "frame {sequence}"
        );
    }
    let stats = painter.render_stats();
    assert!(stats.cache_hits > 0);
    assert!(stats.damaged_pixels < 8 * SURFACE.pixel_count());
    Ok(())
}

#[test]
fn a_leaf_without_layers_paints_past_its_footprint() -> Result<(), TestError> {
    let fonts = crate::test_font::test_font()?;
    let mut style = probe_style();
    style.color = Color::rgb(0);
    let rect = Rect::new(10.dp(), 10.dp(), 100.dp(), 4.dp());
    let text = DrawCommand::Text {
        text: "Lavashell".into(),
        rect,
        style,
    };
    let tree = tree([insert(1, None, 0, leaf(vec![text])?)])?;
    let mut painter = Painter::new(&fonts, crate::UniformRange::VULKAN_REQUIRED)?;
    let mut target = SURFACE.raster()?;
    painter.paint_scene(
        target.canvas(),
        TEST_SURFACE,
        tree.at(Duration::ZERO),
        Density::ONE,
    )?;
    let footprint = rect.outset(4.dp());
    let mut outside = 0;
    for ((x, y), pixel) in pixels(&mut target).positioned()? {
        let at = Point::new(Dp::new(f64::from(x)), Dp::new(f64::from(y)));
        if pixel[3] > 0 && !footprint.contains(at) {
            outside += 1;
        }
    }
    assert!(outside > 0);
    Ok(())
}
