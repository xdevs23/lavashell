// SPDX-License-Identifier: GPL-3.0-only
use super::scene_tree_tests::{SURFACE, leaf, shapes};
use crate::effect_registry::{
    DrawTarget, EffectBackend, EffectRegistry, EffectRequest, QualityChange,
};
use crate::test_error::TestError;
use crate::test_scenes::insert;
use crate::test_variants::only_variant;
use crate::{Fonts, Painter};
use lavashell_core::prelude::*;
use lavashell_core::scene::tree::*;
use lavashell_core::surface::{PerSurface, SurfaceId};
use lavashell_effect_quality::*;
use skia_safe::Canvas;
use std::{cell::RefCell, rc::Rc, time::Duration};

#[derive(Clone, Default)]
struct Recorded(Rc<RefCell<Vec<DrawId>>>);

impl EffectBackend for Recorded {
    fn variants(&self, _: &DrawCommand, _: DrawTarget) -> Vec<OfferedVariant> {
        only_variant(1)
    }

    fn render(&mut self, _: &Canvas, request: &EffectRequest<'_>) {
        self.0.borrow_mut().push(request.id);
    }
}

impl QualityChooser for Recorded {
    fn choose(&mut self, _: FrameBudget, draws: &[EffectDraw]) -> FramePlan {
        *self.0.borrow_mut() = draws.iter().map(|draw| draw.id).collect();
        FramePlan::for_draws(
            draws,
            |draw| draw.id,
            |draw| draw.best_option().map_or(Planned::Wait, Planned::Draw),
        )
    }
}

impl PerSurface for Recorded {
    fn forget_surface(&mut self, _: SurfaceId) {}
}

#[test]
fn effects_in_leaves_are_planned_and_rendered_under_the_key_of_their_leaf() -> Result<(), TestError>
{
    let (planned, rendered) = (Recorded::default(), Recorded::default());
    let mut painter = Painter::new(&Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)?;
    painter.replace_effect_registry(EffectRegistry::empty());
    painter.effect_registry_mut().register_effect(
        EffectId(907),
        |command| matches!(command, DrawCommand::Circle { .. }),
        rendered.clone(),
    )?;
    painter.effect_quality(planned.clone());
    let mut scene = SceneTree::default();
    let guest = scene.create_namespace();
    scene.apply(
        guest,
        SceneUpdate::from_iter([insert(1, None, 0, leaf(shapes())?)]),
    )?;
    let root = [
        insert(1, None, 0, leaf(shapes())?),
        insert(2, None, 1, Node::Leaf(Content::Graft(guest))),
    ];
    scene.show(SceneUpdate::from_iter(root))?;
    let budget = FrameBudget {
        surface: SurfaceId::numbered(3),
        gpu_time: Some(Duration::from_millis(1)),
    };
    let quality =
        painter.plan_scene_effects(scene.at(Duration::ZERO), budget, 1, 1, Affine::IDENTITY);
    assert_eq!(quality, QualityChange::Unchanged);
    let mut target = SURFACE.raster()?;
    painter.paint_scene(
        target.canvas(),
        budget.surface,
        scene.at(Duration::ZERO),
        Density::ONE,
    )?;
    let planned = planned.0.borrow();
    assert_eq!(planned.len(), 2);
    assert_ne!(planned[0], planned[1], "the same node in two namespaces");
    assert_eq!(*rendered.0.borrow(), *planned);
    Ok(())
}
