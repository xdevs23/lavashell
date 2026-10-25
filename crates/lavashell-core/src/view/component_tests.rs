// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::animation::Animation;
use crate::image::{Anchor, ImageFit, ImagePlacement};
use crate::reactive::State;
use crate::test_error::{Exactly, Required, TestResult};
use crate::ui::{Modifier, TextFieldOptions};
use crate::units::{Density, DpExt, VhExt, VwExt};
use std::{cell::Cell, rc::Rc};
use test_scene::{Scene, advance_with};
use test_text::repeated;

const WINDOW: Viewport = Viewport::new(Size::new(Dp::new(300.0), Dp::new(100.0)), Density::ONE);

struct Measure;

impl TextMeasurer for Measure {
    fn measure(&mut self, text: &str, _: &TextStyle<Dp>, _: Dp) -> Size {
        Size::new(repeated(8.dp(), text.len()), 16.dp())
    }
}

#[test]
fn only_dirty_component_recomposes_and_other_packet_survives() -> TestResult {
    let value = State::new(0);
    let value_read = value.clone();
    let builds = Rc::new(Cell::new(0));
    let counter = builds.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let read = value_read.clone();
        ui.layout(Axis::Horizontal, Modifier::new(), |ui| {
            ui.scoped("counter", Modifier::new(), move |ui| {
                ui.text(read.get().to_string(), Modifier::new());
            });
            let counter = counter.clone();
            ui.scoped("static", Modifier::new(), move |ui| {
                counter.set(counter.get() + 1);
                ui.text("static", Modifier::new());
            });
        });
    });
    let window = Viewport::new(Size::new(300.dp(), 100.dp()), Density::ONE);
    advance_with(&mut scene, &mut view, window, &mut Measure)?;
    let before = view.stats();
    value.set(1);
    assert!(view.needs_frame());
    advance_with(&mut scene, &mut view, window, &mut Measure)?;
    assert_eq!(view.stats().root_compositions, before.root_compositions);
    assert_eq!(view.stats().component_compositions, 1);
    assert_eq!(builds.get(), 1);
    assert!(view.stats().reused_components > before.reused_components);
    Ok(())
}

#[test]
fn selector_suppresses_unrelated_updates_and_expires_with_its_owner() {
    let state = State::new((1, 2));
    let selected = state.select(|s| s.0);
    let scope = Scope::default();
    assert_eq!(scope.track(Phase::Composition, || selected.get()), 1);
    state.update(|s| s.1 = 3);
    assert!(!scope.dirty().any());
    state.update(|s| s.0 = 4);
    assert!(scope.dirty().contains(Phase::Composition));
    assert_eq!(selected.get(), 4);
    drop(selected);
    state.set((5, 6));
}

#[test]
fn recycled_hit_input_indices_follow_stable_identity() -> TestResult {
    let value = State::new(false);
    let read = value.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let read = read.clone();
        ui.scoped("growing", Modifier::new(), move |ui| {
            if read.get() {
                ui.text("added", Modifier::new());
            }
        });
        ui.scoped("editor", Modifier::new(), |ui| {
            ui.text_input(
                "text",
                TextFieldOptions::default(),
                Modifier::new().tag("editor"),
                |_| {},
            );
        });
    });
    advance_with(&mut scene, &mut view, WINDOW, &mut Measure)?;
    value.set(true);
    advance_with(&mut scene, &mut view, WINDOW, &mut Measure)?;
    for hit in &view.hits {
        if let Some(index) = hit.input {
            assert_eq!(view.nodes[index].id, hit.id);
        }
    }
    Ok(())
}

#[test]
fn nested_layout_signal_bypasses_an_unchanged_outer_cache() -> TestResult {
    let width = State::new(20.dp());
    let read = width.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let read = read.clone();
        ui.scoped("outer", Modifier::new(), move |ui| {
            let read = read.clone();
            ui.scoped("inner", Modifier::new(), move |ui| {
                let read = read.clone();
                ui.canvas(Size::new(10.dp(), 10.dp()), Modifier::new(), move |d| {
                    d.fill(Rect::new(0.dp(), 0.dp(), read.get(), 10.dp()), Color::WHITE);
                });
            });
        });
    });
    let window = Viewport::new(Size::new(300.dp(), 100.dp()), Density::ONE);
    advance_with(&mut scene, &mut view, window, &mut Measure)?;
    let inner = view
        .nodes
        .iter()
        .filter_map(|n| n.component.as_ref())
        .next_back()
        .required("a node with a component")?
        .scope
        .clone();
    inner.track(Phase::Layout, || width.track());
    width.set(40.dp());
    assert!(inner.dirty().contains(Phase::Layout));
    advance_with(&mut scene, &mut view, window, &mut Measure)?;
    assert!(!inner.dirty().contains(Phase::Layout));
    Ok(())
}

#[test]
fn scoped_shortcuts_follow_replacement_and_removal() -> TestResult {
    let present = State::new(true);
    let read = present.clone();
    let count = Rc::new(Cell::new(0));
    let callback = count.clone();
    let key = Shortcut::control(Key::Character('k'));
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let read = read.clone();
        let callback = callback.clone();
        ui.scoped("shortcuts", Modifier::new(), move |ui| {
            if read.get() {
                let callback = callback.clone();
                ui.on_shortcut(key, move || callback.set(callback.get() + 1));
            }
        });
    });
    let window = Viewport::new(Size::new(300.dp(), 100.dp()), Density::ONE);
    advance_with(&mut scene, &mut view, window, &mut Measure)?;
    view.input(InputEvent::Shortcut(key));
    assert_eq!(count.get(), 1);
    present.set(false);
    advance_with(&mut scene, &mut view, window, &mut Measure)?;
    view.input(InputEvent::Shortcut(key));
    assert_eq!(count.get(), 1);
    present.set(true);
    advance_with(&mut scene, &mut view, window, &mut Measure)?;
    view.input(InputEvent::Shortcut(key));
    assert_eq!(count.get(), 2);
    Ok(())
}

#[test]
fn a_retained_image_moves_with_the_window_height_by_an_offset_in_vh() -> TestResult {
    let image = crate::image::Image::encoded(Vec::new(), 1400, 1000);
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let image = image.clone();
        ui.scoped("wallpaper", Modifier::new(), move |ui| {
            let image = image.clone();
            ui.canvas(Size::new(64.dp(), 32.dp()), Modifier::new(), move |d| {
                let fit = ImageFit::Cover(ImagePlacement::at(Anchor::Top).offset(0.vw(), 10.vh()));
                d.image(&image, Rect::new(0.dp(), 0.dp(), 64.dp(), 32.dp()), fit);
            });
        });
    });
    let tops = |scene: &Scene| {
        (scene.commands().into_iter())
            .filter_map(|command| match command {
                DrawCommand::Image { frame, .. } => Some(frame.y),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    let mut scene = Scene::default();
    let window = Size::new(300.dp(), 100.dp());
    scene.show(view.frame(Viewport::new(window, Density::ONE), &mut Measure)?)?;
    assert_eq!(tops(&scene), [10.dp()]);
    let resized = Size::new(400.dp(), 200.dp());
    scene.show(view.frame(Viewport::new(resized, Density::ONE), &mut Measure)?)?;
    assert_eq!(tops(&scene), [20.dp()]);
    Ok(())
}

#[test]
fn an_offset_lays_out_again_only_the_component_nearest_to_its_node() -> TestResult {
    let shift = Animation::new(0.dp())?;
    let read = shift.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let read = read.clone();
        ui.scoped("outer", Modifier::new(), move |ui| {
            let read = read.clone();
            ui.scoped("inner", Modifier::new(), move |ui| {
                ui.spacer(10.dp(), 10.dp(), Modifier::new().offset(&read, 0.dp()));
            });
        });
    });
    advance_with(&mut scene, &mut view, WINDOW, &mut Measure)?;
    let scopes: Vec<_> = (view.nodes.iter())
        .filter_map(|node| {
            node.component
                .as_ref()
                .map(|component| component.scope.clone())
        })
        .collect();
    let [outer, inner] = scopes.exactly::<2>("composed components")?;
    shift.snap(10.dp())?;
    assert!(!outer.dirty().contains(Phase::Layout));
    assert!(inner.dirty().contains(Phase::Layout));
    Ok(())
}
