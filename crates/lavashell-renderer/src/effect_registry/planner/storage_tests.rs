// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::effect_registry::{EffectBackend, EffectInput, EffectRegistry, QualityChange};
use crate::test_error::TestError;
use lavashell_core::retained::Footprint;
use lavashell_core::{prelude::*, protected_text::ProtectedText};
use lavashell_effect_quality::BudgetChooser;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Duration,
};

struct Recorder {
    inputs: Rc<RefCell<Vec<EffectInput>>>,
    retired: Rc<Cell<Option<SurfaceId>>>,
}

impl EffectBackend for Recorder {
    fn variants(&self, _: &DrawCommand, _: DrawTarget) -> Vec<VariantId> {
        vec![VariantId(1)]
    }

    fn render(&mut self, _: &Canvas, request: &EffectRequest<'_>) {
        if let Some(input) = request.input {
            self.inputs.borrow_mut().push(input.clone());
        }
    }

    fn forget_surface(&mut self, surface: SurfaceId) {
        self.retired.set(Some(surface));
    }
}

/// Plans `tree` as frame and scene generation `frame` of surface 30, without GPU time to spend.
fn plan(painter: &mut Painter, tree: &SceneTree, frame: u64) -> QualityChange {
    let budget = FrameBudget {
        surface: SurfaceId(30),
        gpu_time: Some(Duration::ZERO),
    };
    let at = tree.at(Duration::ZERO);
    painter.plan_scene_effects(at, budget, frame, frame, Density::ONE)
}

#[test]
fn private_prefixes_are_never_held_in_effect_inputs_or_reusable_backdrop_snapshots()
-> Result<(), TestError> {
    let mut painter = Painter::new(&crate::Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)?;
    let secret = ProtectedText::new();
    let state = secret.clone();
    let mut view = View::new(lavashell_test_platform::facts(), move |ui| {
        ui.protected_input(
            state.clone(),
            TextFieldOptions::default(),
            Modifier::new().size(80.dp(), 30.dp()),
        );
    });
    let size = Size::new(100.dp(), 40.dp());
    let viewport = Viewport::new(size, Density::ONE);
    let mut tree = lavashell_core::scene::tree::SceneTree::default();
    tree.show(view.frame(viewport, &mut painter)?)?;
    view.input(InputEvent::Key {
        key: Key::Tab,
        shift: false,
    });
    view.input(InputEvent::Text("synthetic fixture".into()));
    secret.set_revealed(true);
    tree.show(view.frame(viewport, &mut painter)?)?;
    let private = crate::test_scenes::commands(&tree);
    assert!(!Footprint::analyze(&private).cacheable);
    let inputs = Rc::new(RefCell::new(Vec::new()));
    let retired = Rc::new(Cell::new(None));
    painter.effect_registry = EffectRegistry::empty();
    painter.effect_registry_mut().register_effect(
        EffectId(3000),
        |command| matches!(command, DrawCommand::Circle { .. }),
        Recorder {
            inputs: inputs.clone(),
            retired: retired.clone(),
        },
    )?;
    painter.effect_quality(BudgetChooser::default());
    let circle = DrawCommand::Circle {
        center: Point::new(10.dp(), 10.dp()),
        radius: 4.dp(),
        brush: Color::WHITE.into(),
    };
    let ordinary = [DrawCommand::Fill {
        shape: Rect::new(0.dp(), 0.dp(), 100.dp(), 40.dp()).into(),
        brush: Color::WHITE.into(),
    }];
    let mut surface =
        skia_safe::surfaces::raster_n32_premul((100, 40)).ok_or(TestError::RasterSurface)?;
    for (prefix, cacheable) in [(&ordinary[..], true), (&private[..], false)] {
        let mut commands = prefix.to_vec();
        commands.push(circle.clone());
        let tree = SceneTree::with_leaf(commands)?;
        assert_eq!(plan(&mut painter, &tree, 1), QualityChange::Unchanged);
        painter.render_effect(
            surface.canvas(),
            &circle,
            Destination::Surface,
            SceneTree::LEAF,
            0,
        );
        let input = inputs
            .borrow_mut()
            .pop()
            .ok_or(TestError::Missing("recorded effect input"))?;
        assert_eq!(input.cache_allowed, cacheable);
    }
    let mut commands = ordinary.to_vec();
    commands.push(circle.clone());
    let tree = SceneTree::with_leaf(commands)?;
    assert_eq!(plan(&mut painter, &tree, 2), QualityChange::Unchanged);
    if !Footprint::analyze(&private).cacheable {
        painter.forget_transient_effect_inputs();
    }
    painter.render_effect(
        surface.canvas(),
        &circle,
        Destination::Surface,
        SceneTree::LEAF,
        0,
    );
    let input = inputs
        .borrow_mut()
        .pop()
        .ok_or(TestError::Missing("recorded effect input"))?;
    assert!(!input.cache_allowed);
    assert_eq!(retired.get(), Some(SurfaceId(30)));
    Ok(())
}
