// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::local::Local;
use crate::platform::PASSWORD_MASK;
use crate::preferences::{TEXT_SCALING, TextScaling};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

static NAME: Local<&str> = Local::new(|| "default");
static OTHER_NAME: Local<&str> = Local::new(|| "other default");
static INK: Local<Color> = Local::new(|| Color::WHITE);

const WINDOW: Viewport = Viewport::new(Size::new(Dp::new(300.0), Dp::new(100.0)), Density::ONE);

fn texts(scene: &Scene) -> Vec<String> {
    (scene.commands().into_iter())
        .filter_map(|command| match command {
            DrawCommand::Text { text, .. } => Some(text),
            _ => None,
        })
        .collect()
}

#[test]
fn the_nearest_wrapper_wins_and_a_key_without_one_reads_its_default() -> TestResult {
    let seen = Rc::new(RefCell::new(Vec::new()));
    let record = seen.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let read = |ui: &Ui| {
            record
                .borrow_mut()
                .push((ui.local(&NAME), ui.local(&OTHER_NAME)));
        };
        read(ui);
        ui.provide(&NAME, "outer", |ui| {
            read(ui);
            ui.provide(&NAME, "inner", |ui| read(ui));
            read(ui);
        });
        read(ui);
    });
    advance_with(&mut scene, &mut view, WINDOW, &mut Metrics)?;
    let expected =
        ["default", "outer", "inner", "outer", "default"].map(|name| (name, "other default"));
    assert_eq!(*seen.borrow(), expected);
    Ok(())
}

#[test]
fn a_root_value_composes_again_only_the_components_that_read_it() -> TestResult {
    let (reader, bystander) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let (reads, stays) = (reader.clone(), bystander.clone());
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let (reads, stays) = (reads.clone(), stays.clone());
        ui.layout(Axis::Horizontal, Modifier::new(), move |ui| {
            ui.scoped("reader", Modifier::new(), move |ui| {
                reads.set(reads.get() + 1);
                ui.text(ui.local(&NAME), Modifier::new());
            });
            ui.scoped("bystander", Modifier::new(), move |ui| {
                stays.set(stays.get() + 1);
                ui.text("static", Modifier::new());
            });
        });
    });
    let mut scene = Scene::default();
    scene.show(view.frame(WINDOW, &mut Metrics).without_failures()?)?;
    let before = view.stats();
    view.provide(&NAME, "changed");
    assert!(view.needs_frame());
    scene.show(view.frame(WINDOW, &mut Metrics).without_failures()?)?;
    assert_eq!(texts(&scene), ["changed", "static"]);
    assert_eq!((reader.get(), bystander.get()), (2, 1));
    assert_eq!(view.stats().root_compositions, before.root_compositions);
    assert_eq!(view.stats().reused_components, before.reused_components + 1);
    view.provide(&NAME, "changed");
    assert!(!view.needs_frame());
    Ok(())
}

#[test]
fn a_platform_key_reads_the_facts_and_follows_them_when_they_are_replaced() -> TestResult {
    let (reader, bystander) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let (reads, stays) = (reader.clone(), bystander.clone());
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let (reads, stays) = (reads.clone(), stays.clone());
        ui.layout(Axis::Horizontal, Modifier::new(), move |ui| {
            ui.scoped("reader", Modifier::new(), move |ui| {
                reads.set(reads.get() + 1);
                ui.text(ui.local(&PASSWORD_MASK).to_string(), Modifier::new());
            });
            ui.scoped("bystander", Modifier::new(), move |ui| {
                stays.set(stays.get() + 1);
                ui.text("static", Modifier::new());
            });
        });
    });
    let mut scene = Scene::default();
    scene.show(view.frame(WINDOW, &mut Metrics).without_failures()?)?;
    assert_eq!(texts(&scene), ["•", "static"]);
    view.provide_platform(PlatformFacts {
        password_mask: '*',
        ..crate::platform::fixture::facts()
    });
    scene.show(view.frame(WINDOW, &mut Metrics).without_failures()?)?;
    assert_eq!(texts(&scene), ["*", "static"]);
    assert_eq!((reader.get(), bystander.get()), (2, 1));
    assert_eq!(view.platform().password_mask, '*');
    view.provide_platform(view.platform());
    assert!(!view.needs_frame());
    Ok(())
}

#[test]
fn a_root_value_provided_for_a_platform_key_outlasts_new_facts() -> TestResult {
    let mut view = View::new(crate::platform::fixture::facts(), |ui| {
        ui.text(ui.local(&PASSWORD_MASK).to_string(), Modifier::new());
    });
    view.provide(&PASSWORD_MASK, '#');
    let mut scene = Scene::default();
    scene.show(view.frame(WINDOW, &mut Metrics).without_failures()?)?;
    view.provide_platform(PlatformFacts {
        password_mask: '*',
        ..crate::platform::fixture::facts()
    });
    assert!(!view.needs_frame());
    scene.show(view.frame(WINDOW, &mut Metrics).without_failures()?)?;
    assert_eq!(texts(&scene), ["#"]);
    Ok(())
}

#[test]
fn a_retained_field_paints_again_when_the_caret_width_of_the_platform_changes() -> TestResult {
    let mut view = View::new(crate::platform::fixture::facts(), |ui| {
        ui.scoped("field", Modifier::new(), |ui| {
            ui.text_input("text", TextFieldOptions::default(), Modifier::new(), |_| {});
        });
    });
    let mut scene = Scene::default();
    scene.show(view.frame(WINDOW, &mut Metrics).without_failures()?)?;
    view.provide_platform(PlatformFacts {
        caret_width: Some(3.dp()),
        ..crate::platform::fixture::facts()
    });
    assert!(view.needs_visual_frame());
    Ok(())
}

#[test]
fn a_scoped_component_reads_the_wrappers_around_it_when_it_composes_again_alone() -> TestResult {
    let signal = State::new(0);
    let seen = Rc::new(RefCell::new(Vec::new()));
    let (read, record) = (signal.clone(), seen.clone());
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let (read, record) = (read.clone(), record.clone());
        ui.provide(&NAME, "wrapped", move |ui| {
            ui.scoped("reader", Modifier::new(), move |ui| {
                read.track();
                record.borrow_mut().push(ui.local(&NAME));
            });
        });
    });
    advance_with(&mut scene, &mut view, WINDOW, &mut Metrics)?;
    signal.set(1);
    advance_with(&mut scene, &mut view, WINDOW, &mut Metrics)?;
    assert_eq!(*seen.borrow(), ["wrapped", "wrapped"]);
    assert_eq!(view.stats().root_compositions, 1);
    Ok(())
}

#[test]
fn a_component_created_again_with_a_changed_value_repaints_while_others_keep_their_paint()
-> TestResult {
    let red = Color::oklch(0.6, 0.2, 30.0);
    let ink = State::new(Color::WHITE);
    let source = ink.clone();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let source = source.clone();
        ui.scoped("outer", Modifier::new(), move |ui| {
            ui.provide(&INK, source.get(), |ui| {
                let square = Rect::new(0.dp(), 0.dp(), 10.dp(), 10.dp());
                ui.scoped("reader", Modifier::new(), move |ui| {
                    let ink = ui.local(&INK);
                    ui.canvas(square.size(), Modifier::new(), move |d| d.fill(square, ink));
                });
                ui.scoped("bystander", Modifier::new(), move |ui| {
                    ui.canvas(square.size(), Modifier::new(), move |d| {
                        d.fill(square, Color::TRANSPARENT);
                    });
                });
            });
        });
    });
    let fills = |scene: &Scene| {
        (scene.commands().into_iter())
            .filter_map(|command| match command {
                DrawCommand::Fill { brush, .. } => Some(brush),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    let mut scene = Scene::default();
    scene.show(view.frame(WINDOW, &mut Metrics).without_failures()?)?;
    let before = view.stats();
    ink.set(red);
    scene.show(view.frame(WINDOW, &mut Metrics).without_failures()?)?;
    assert_eq!(fills(&scene), [red.into(), Brush::from(Color::TRANSPARENT)]);
    assert_eq!(view.stats().reused_components, before.reused_components + 1);
    Ok(())
}

#[test]
fn a_component_created_again_under_an_equal_state_style_keeps_its_paint() -> TestResult {
    use crate::style::{INTERACTION_STYLE, InteractionAppearance, InteractionStyle, StateStyles};
    let style = InteractionStyle {
        states: Some(StateStyles::new(|_| InteractionAppearance::default())),
        ..InteractionStyle::default()
    };
    let signal = State::new(0);
    let read = signal.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let (read, style) = (read.clone(), style.clone());
        ui.scoped("outer", Modifier::new(), move |ui| {
            read.track();
            ui.provide(&INTERACTION_STYLE, style.clone(), |ui| {
                ui.scoped("button", Modifier::new(), |ui| {
                    ui.text("press", Modifier::new());
                });
            });
        });
    });
    advance_with(&mut scene, &mut view, WINDOW, &mut Metrics)?;
    let before = view.stats();
    signal.set(1);
    advance_with(&mut scene, &mut view, WINDOW, &mut Metrics)?;
    assert_eq!(view.stats().reused_components, before.reused_components + 1);
    Ok(())
}

#[test]
fn a_shared_subtree_moved_under_another_wrapper_reads_its_value_and_keeps_its_state() -> TestResult
{
    let first = State::new(true);
    let seen = Rc::new(RefCell::new(Vec::new()));
    let created = Rc::new(Cell::new(0));
    let (side, record, count) = (first.clone(), seen.clone(), created.clone());
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let moving = |ui: &mut Ui| {
            ui.shared("moving", Modifier::new(), |ui| {
                let _remembered_state = ui.state("remembered", || count.set(count.get() + 1));
                record.borrow_mut().push(ui.local(&NAME));
            });
        };
        let first = side.get();
        ui.provide(&NAME, "first", |ui| {
            if first {
                moving(ui);
            }
        });
        ui.provide(&NAME, "second", |ui| {
            if !first {
                moving(ui);
            }
        });
    });
    advance_with(&mut scene, &mut view, WINDOW, &mut Metrics)?;
    first.set(false);
    advance_with(&mut scene, &mut view, WINDOW, &mut Metrics)?;
    assert_eq!(*seen.borrow(), ["first", "second"]);
    assert_eq!(created.get(), 1);
    Ok(())
}

#[test]
fn a_wrapper_sets_the_text_size_and_the_viewport_for_its_part_of_the_view() -> TestResult {
    let root = Viewport::new(WINDOW.size, Density::from_scale(2.0).required("a density")?);
    let window = Viewport::new(Size::new(100.dp(), 50.dp()), root.density);
    let read = Rc::new(RefCell::new(Vec::new()));
    let drawn = Rc::new(RefCell::new(Vec::new()));
    let (record, draw) = (read.clone(), drawn.clone());
    let doubled = TextScaling::linear(2.0).required("a text scaling")?;
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        record.borrow_mut().push(ui.viewport());
        let draw = draw.clone();
        ui.layout(Axis::Vertical, Modifier::new(), |ui| {
            ui.provide(&TEXT_SCALING, doubled.clone(), |ui| {
                ui.text("large", Modifier::new().tag("large"));
            });
            ui.text("plain", Modifier::new().tag("plain"));
            ui.provide(&VIEWPORT, window, |ui| {
                ui.canvas(Size::new(10.dp(), 10.dp()), Modifier::new(), move |d| {
                    draw.borrow_mut().push(d.context.viewport);
                });
            });
        });
    });
    advance_with(&mut scene, &mut view, root, &mut Metrics)?;
    let (large, plain) = (view.tagged_bounds("large")?, view.tagged_bounds("plain")?);
    assert_eq!(large.height, plain.height * 2.0);
    assert_eq!(*read.borrow(), [root]);
    assert_eq!(*drawn.borrow(), [window]);
    Ok(())
}
