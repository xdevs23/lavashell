// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::effect_registry::QualityChange;
use crate::field::test_shapes::union;
use crate::test_error::TestError;
use crate::test_scenes::{insert, paint};
use crate::{Fonts, Painter};
use lavashell_core::prelude::*;
use lavashell_core::scene::tree::{Content, Group, Node, Packet, SceneTree, SceneUpdate};
use lavashell_effect_quality::*;
use std::{cell::RefCell, rc::Rc, sync::Arc, time::Duration};

type Recorded = Rc<RefCell<Vec<(DrawId, Option<u64>)>>>;

struct Recorder(Recorded);

impl EffectBackend for Recorder {
    fn variants(&self, _: &DrawCommand, _: DrawTarget) -> Vec<VariantId> {
        vec![VariantId(1)]
    }

    fn render(&mut self, _: &Canvas, request: &EffectRequest<'_>) {
        self.0.borrow_mut().push((
            request.id,
            request.input.map(|input| input.scene_generation),
        ));
    }
}

/// An effect backend that offers variant 1 where a dp spans at least one device pixel and variant 2 everywhere. It
/// records each variant that it draws.
struct VariantRecorder(Rc<RefCell<Vec<VariantId>>>);

impl EffectBackend for VariantRecorder {
    fn variants(&self, _: &DrawCommand, target: DrawTarget) -> Vec<VariantId> {
        let fine = (target.pixels_per_dp >= 1.0).then_some(VariantId(1));
        fine.into_iter().chain([VariantId(2)]).collect()
    }

    fn render(&mut self, _: &Canvas, request: &EffectRequest<'_>) {
        self.0.borrow_mut().push(request.variant);
    }
}

/// A painter with an effect for circles whose best variant needs a dp to span a device pixel, and the draw commands
/// of a circle for that effect.
fn fine_effect(
    draws: &Rc<RefCell<Vec<VariantId>>>,
) -> Result<(Painter, Vec<DrawCommand>), TestError> {
    let mut painter = Painter::new(&Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)?;
    painter.effect_registry = EffectRegistry::empty();
    painter.effect_registry_mut().register_effect(
        EffectId(905),
        |c| matches!(c, DrawCommand::Circle { .. }),
        VariantRecorder(draws.clone()),
    )?;
    painter.effect_quality(BudgetChooser::default());
    let mut commands = Vec::new();
    let context = DrawContext::new(Viewport::new(Size::new(10.dp(), 10.dp()), Density::ONE));
    Drawing::new(
        &mut commands,
        Rect::new(0.dp(), 0.dp(), 10.dp(), 10.dp()),
        &context,
    )
    .circle(Point::new(5.dp(), 5.dp()), 3.dp(), Color::WHITE);
    Ok((painter, commands))
}

fn budget() -> FrameBudget {
    FrameBudget {
        surface: SurfaceId(0),
        gpu_time: Some(Duration::ZERO),
    }
}

#[test]
fn an_unplanned_painter_paints_on_the_unspecified_surface_until_a_frame_names_another()
-> Result<(), TestError> {
    let mut painter = Painter::new(&Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)?;
    assert_eq!(
        painter.effect_planner.current_surface(),
        SurfaceId::UNSPECIFIED
    );
    let budget = FrameBudget {
        surface: SurfaceId(7),
        ..budget()
    };
    let empty = SceneTree::default();
    assert_eq!(
        painter.plan_scene_effects(empty.at(Duration::ZERO), budget, 1, 1, Density::ONE),
        QualityChange::Unchanged
    );
    assert_eq!(painter.effect_planner.current_surface(), SurfaceId(7));
    Ok(())
}

#[test]
fn a_planned_variant_that_the_canvas_cannot_draw_falls_back_to_the_best_one_it_can()
-> Result<(), TestError> {
    let draws = Rc::new(RefCell::new(Vec::new()));
    let (mut painter, commands) = fine_effect(&draws)?;
    let tree = SceneTree::with_leaf(&commands[..])?;
    assert_eq!(
        painter.plan_scene_effects(tree.at(Duration::ZERO), budget(), 1, 1, Density::ONE),
        QualityChange::Unchanged
    );
    let mut halved =
        skia_safe::surfaces::raster_n32_premul((10, 10)).ok_or(TestError::RasterSurface)?;
    let _chained_canvas = halved.canvas().scale((0.5, 0.5));
    painter.render_effect(
        halved.canvas(),
        &commands[0],
        Destination::Surface,
        SceneTree::LEAF,
        0,
    );
    assert_eq!(*draws.borrow(), [VariantId(2)]);
    Ok(())
}

struct PlanRecorder(Rc<RefCell<Vec<DrawId>>>);

#[test]
fn alternating_windows_keep_distinct_draw_identity_and_unchanged_pixels() -> Result<(), TestError> {
    let draws = Rc::new(RefCell::new(Vec::new()));
    let mut painter = Painter::new(&Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)?;
    painter.effect_registry = EffectRegistry::empty();
    painter.effect_registry_mut().register_effect(
        EffectId(905),
        |c| matches!(c, DrawCommand::Circle { .. }),
        Recorder(draws.clone()),
    )?;
    painter.effect_quality(BudgetChooser::default());
    let mut commands = Vec::new();
    let context = DrawContext::new(Viewport::new(Size::new(10.dp(), 10.dp()), Density::ONE));
    Drawing::new(
        &mut commands,
        Rect::new(0.dp(), 0.dp(), 10.dp(), 10.dp()),
        &context,
    )
    .circle(Point::new(5.dp(), 5.dp()), 3.dp(), Color::WHITE);
    let tree = SceneTree::with_leaf(commands)?;
    let mut surface =
        skia_safe::surfaces::raster_n32_premul((10, 10)).ok_or(TestError::RasterSurface)?;
    for (frame, window) in [(1, 11), (1, 21), (2, 11), (2, 21)] {
        let budget = FrameBudget {
            surface: SurfaceId(window),
            gpu_time: Some(Duration::ZERO),
        };
        assert_eq!(
            painter.plan_scene_effects(tree.at(Duration::ZERO), budget, frame, 1, Density::ONE),
            QualityChange::Unchanged,
            "switching windows must not invalidate unchanged choices"
        );
        paint(&mut painter, surface.canvas(), &tree, Density::ONE);
    }
    let draws = draws.borrow();
    assert_ne!(draws[0].0, draws[1].0);
    assert_eq!(draws[0].0, draws[2].0);
    assert_eq!(draws[1].0, draws[3].0);
    painter.forget_effect_surface(SurfaceId(11));
    Ok(())
}

impl QualityChooser for PlanRecorder {
    fn choose(&mut self, _: FrameBudget, draws: &[EffectDraw]) -> FramePlan {
        *self.0.borrow_mut() = draws.iter().map(|draw| draw.id).collect();
        FramePlan::for_draws(
            draws,
            |draw| draw.id,
            |draw| (draw.options.first()).map_or(Planned::Wait, |best| Planned::Draw(*best)),
        )
    }
}

#[test]
fn mixed_leaf_registrations_match_planning_ids_across_leaves() -> Result<(), TestError> {
    let draws = Rc::new(RefCell::new(Vec::new()));
    let planned = Rc::new(RefCell::new(Vec::new()));
    let mut painter = Painter::new(&Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)?;
    painter.effect_registry = EffectRegistry::empty();
    for (id, matcher) in [
        (
            901,
            (|c: &DrawCommand| matches!(c, DrawCommand::Shadow { .. })) as Matcher,
        ),
        (
            902,
            (|c: &DrawCommand| matches!(c, DrawCommand::Circle { .. })) as Matcher,
        ),
    ] {
        painter.effect_registry_mut().register_effect(
            EffectId(id),
            matcher,
            Recorder(draws.clone()),
        )?;
    }
    painter.set_packet_caching(crate::PacketCaching::Disabled);
    painter.effect_quality(PlanRecorder(planned.clone()));
    let rect = Rect::new(0.dp(), 0.dp(), 50.dp(), 50.dp());
    let mut leaves = Vec::new();
    let context = DrawContext::new(Viewport::new(rect.size(), Density::ONE));
    let mut drawing = Drawing::new(&mut leaves, rect, &context);
    drawing.shadow(
        rect,
        Shadow::new(Color::WHITE, 1.dp(), Point::new(0.dp(), 0.dp()))?,
    );
    drawing.circle(Point::new(0.dp(), 0.dp()), 3.dp(), Color::WHITE);
    let packet = |commands: Vec<DrawCommand>| Packet::new(Arc::from(commands));
    let leaf = |packet| Node::Leaf(Content::Packet(packet));
    let changes = vec![
        insert(1, None, 0, leaf(packet(vec![leaves[0].clone()]))),
        insert(2, None, 1, Node::Group(Group::default())),
        insert(3, Some(2), 0, leaf(packet(leaves.clone()))),
        insert(4, None, 2, leaf(packet(vec![leaves[1].clone()]))),
    ];
    let mut tree = SceneTree::default();
    tree.show(SceneUpdate { changes })?;
    assert_eq!(
        painter.plan_scene_effects(
            tree.at(Duration::ZERO),
            FrameBudget {
                surface: SurfaceId(0),
                gpu_time: Some(Duration::from_millis(1)),
            },
            1,
            77,
            Density::ONE,
        ),
        QualityChange::Unchanged
    );
    let mut surface =
        skia_safe::surfaces::raster_n32_premul((100, 100)).ok_or(TestError::RasterSurface)?;
    paint(&mut painter, surface.canvas(), &tree, Density::ONE);
    let observed = draws.borrow();
    assert_eq!(
        observed.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
        *planned.borrow()
    );
    assert_eq!(observed.len(), 4);
    assert!(
        observed
            .iter()
            .all(|(_, generation)| *generation == Some(77))
    );
    assert_eq!(
        painter.plan_scene_effects(
            SceneTree::default().at(Duration::ZERO),
            budget(),
            2,
            77,
            Density::ONE,
        ),
        QualityChange::Changed,
        "removed effects must invalidate retained pixels"
    );
    Ok(())
}

/// Fills a box clipped to two pills that do not touch.
struct PillPainter(Shape);

impl PillPainter {
    fn new() -> Result<Self, TestError> {
        let pill = |x| {
            RoundedRect::new(Rect::new(x, 0.dp(), 10.dp(), 10.dp()), Corners::all(5.dp())).into()
        };
        let pills = union([pill(0.dp()), pill(30.dp())], 0.dp())?;
        Ok(Self(pills))
    }
}

impl EffectBackend for PillPainter {
    fn variants(&self, _: &DrawCommand, _: DrawTarget) -> Vec<VariantId> {
        vec![VariantId(1)]
    }

    fn render(&mut self, canvas: &Canvas, request: &EffectRequest<'_>) {
        let mut paint = skia_safe::Paint::default();
        let _chained_paint = paint.set_color(skia_safe::Color::WHITE);
        let saved = canvas.save();
        let shapes = request.painter;
        let drawn = shapes.clip_inside(canvas, &self.0).and_then(|()| {
            shapes.fill(
                canvas,
                &Shape::rounded(Rect::new(0.dp(), 0.dp(), 40.dp(), 10.dp()), 0.dp()),
                &paint,
            )
        });
        if let Err(failure) = drawn {
            shapes.record(failure);
        }
        let _chained_canvas = canvas.restore_to_count(saved);
    }
}

#[test]
fn effects_fill_and_clip_shapes_the_way_the_painter_draws_them() -> Result<(), TestError> {
    let mut painter = Painter::new(&Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)?;
    painter.effect_registry = EffectRegistry::empty();
    painter.effect_registry_mut().register_effect(
        EffectId(907),
        |c| matches!(c, DrawCommand::Circle { .. }),
        PillPainter::new()?,
    )?;
    let circle = DrawCommand::Circle {
        center: Point::new(5.dp(), 5.dp()),
        radius: 3.dp(),
        brush: Color::WHITE.into(),
    };
    let mut surface =
        skia_safe::surfaces::raster_n32_premul((40, 10)).ok_or(TestError::RasterSurface)?;
    let _chained_canvas = surface.canvas().clear(skia_safe::Color::TRANSPARENT);
    painter.paint_as_leaf(surface.canvas(), &[circle], Density::ONE)?;
    let pixels = surface
        .canvas()
        .peek_pixels()
        .ok_or(TestError::Missing("pixels"))?;
    let alpha = |x| pixels.get_color((x, 5)).a();
    assert_eq!([alpha(5), alpha(20), alpha(35)], [255, 0, 255]);
    Ok(())
}

struct SurfaceRecorder(Rc<RefCell<Vec<SurfaceAccess>>>);

impl EffectBackend for SurfaceRecorder {
    fn variants(&self, _: &DrawCommand, _: DrawTarget) -> Vec<VariantId> {
        vec![VariantId(1)]
    }

    fn render(&mut self, _: &Canvas, request: &EffectRequest<'_>) {
        self.0.borrow_mut().push(request.access);
    }
}

#[test]
fn effects_inside_layers_cannot_read_the_surface() -> Result<(), TestError> {
    let surfaces = Rc::new(RefCell::new(Vec::new()));
    let mut painter = Painter::new(&Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)?;
    painter.effect_registry = EffectRegistry::empty();
    painter.effect_registry_mut().register_effect(
        EffectId(906),
        |c| matches!(c, DrawCommand::Circle { .. }),
        SurfaceRecorder(surfaces.clone()),
    )?;
    let rect = Rect::new(0.dp(), 0.dp(), 10.dp(), 10.dp());
    let circle = DrawCommand::Circle {
        center: Point::new(5.dp(), 5.dp()),
        radius: 3.dp(),
        brush: Color::WHITE.into(),
    };
    let effect_layer = DrawCommand::EffectLayer {
        bounds: rect,
        blend: BlendMode::Multiply,
        blur: NonNegativeDp::ZERO,
        filter: None,
    };
    let commands = [
        circle.clone(),
        DrawCommand::Opacity(1.0),
        circle.clone(),
        DrawCommand::Restore,
        DrawCommand::Opacity(0.5),
        circle.clone(),
        DrawCommand::Save,
        circle.clone(),
        DrawCommand::Restore,
        DrawCommand::Restore,
        effect_layer,
        circle.clone(),
        DrawCommand::Restore,
        DrawCommand::Opacity(0.5),
        circle.clone(),
        DrawCommand::Restore,
        circle,
    ];
    let mut surface =
        skia_safe::surfaces::raster_n32_premul((10, 10)).ok_or(TestError::RasterSurface)?;
    painter.paint_as_leaf(surface.canvas(), &commands, Density::ONE)?;
    let (current, unreadable) = (SurfaceAccess::Current, SurfaceAccess::Unreadable);
    assert_eq!(
        *surfaces.borrow(),
        [
            current, current, unreadable, unreadable, unreadable, unreadable, current
        ]
    );
    painter.set_blend_space(crate::BlendSpace::Srgb);
    surfaces.borrow_mut().clear();
    painter.paint_as_leaf(surface.canvas(), &commands[..1], Density::ONE)?;
    assert_eq!(*surfaces.borrow(), [unreadable]);
    Ok(())
}

#[test]
fn an_effect_registered_twice_is_refused_and_the_first_registration_stays() -> Result<(), TestError>
{
    let draws = Rc::new(RefCell::new(Vec::new()));
    let mut registry = EffectRegistry::empty();
    let circle = |c: &DrawCommand| matches!(c, DrawCommand::Circle { .. });
    let shadow = |c: &DrawCommand| matches!(c, DrawCommand::Shadow { .. });
    registry.register_effect(EffectId(906), circle, VariantRecorder(draws.clone()))?;
    let revision = registry.revision();
    let again = registry.register_effect(EffectId(906), shadow, VariantRecorder(draws));
    assert!(matches!(
        again,
        Err(crate::Error::DuplicateEffect(EffectId(906)))
    ));
    assert_eq!(registry.revision(), revision);
    let (_, commands) = fine_effect(&Rc::new(RefCell::new(Vec::new())))?;
    assert_eq!(registry.identify(&commands[0]), Some(EffectId(906)));
    Ok(())
}
