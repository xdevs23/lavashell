// SPDX-License-Identifier: GPL-3.0-only
use super::*;

/// A row tagged "row" with two forms. The first form is a spacer 120 dp wide, referred to by `wide`, shown while `fits`
/// is true. The second form is a spacer 40 dp wide, shown otherwise.
fn forms(ui: &mut Ui, wide: &NodeRef, fits: &Deferred<bool>) {
    ui.layout(Axis::Horizontal, Modifier::new().tag("row"), |ui| {
        let wide_form = Modifier::new().node_ref(wide).shown_when(fits.clone());
        ui.spacer(120.dp(), 20.dp(), wide_form);
        let narrow_form = Modifier::new().shown_when(fits.map(|fits| !fits));
        ui.spacer(40.dp(), 20.dp(), narrow_form);
    });
}

/// The bounds of the fills in `scene` after it shows a new frame of `view` at `size`.
fn fill_bounds(scene: &mut Scene, view: &mut View, size: Size) -> Result<Vec<Rect>, TestError> {
    advance(scene, view, size, 0)?;
    let bounds = (scene.commands().into_iter())
        .filter_map(|command| match command {
            DrawCommand::Fill { shape, .. } => Some(shape.bounds()),
            _ => None,
        })
        .collect();
    Ok(bounds)
}

#[test]
fn a_hidden_child_takes_no_room_no_gap_and_no_share_in_a_row() -> TestResult {
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), |ui| {
        let row = Modifier::new().width(300.dp()).gap(10.dp());
        ui.layout(Axis::Horizontal, row, |ui| {
            ui.spacer(Dp::ZERO, 20.dp(), Modifier::new().weight(1.0).tag("share"));
            let hidden = Modifier::new().weight(1.0).shown_when(false);
            ui.spacer(40.dp(), 20.dp(), hidden);
            ui.spacer(50.dp(), 20.dp(), Modifier::new().tag("fixed"));
        });
    });
    advance(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    assert_eq!(
        view.tagged_bounds("share")?,
        Rect::new(0.dp(), 0.dp(), 240.dp(), 20.dp())
    );
    assert_eq!(
        view.tagged_bounds("fixed")?,
        Rect::new(250.dp(), 0.dp(), 50.dp(), 20.dp())
    );
    Ok(())
}

#[test]
fn only_the_shown_form_paints_and_takes_a_click() -> TestResult {
    let clicks = State::new(Vec::new());
    let counted = clicks.clone();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        ui.layout(Arrangement::Overlay, Modifier::new(), |ui| {
            for (form, shown, width) in [("wide", true, 100.dp()), ("narrow", false, 60.dp())] {
                let counted = counted.clone();
                let click = move || counted.update(|clicks| clicks.push(form));
                let filled = Modifier::new().background(Color::WHITE).on_click(click);
                ui.spacer(width, 40.dp(), filled.shown_when(shown));
            }
        });
    });
    let fills = fill_bounds(&mut Scene::default(), &mut view, WINDOW_SIZE)?;
    assert_eq!(fills, [Rect::new(0.dp(), 0.dp(), 100.dp(), 40.dp())]);
    tap_at(&mut view, Point::new(30.dp(), 20.dp()));
    assert_eq!(clicks.get(), ["wide"]);
    Ok(())
}

#[test]
fn a_condition_reads_the_natural_width_of_its_own_node() -> TestResult {
    for (room, shown) in [(150.dp(), 120.dp()), (100.dp(), 40.dp())] {
        let mut scene = Scene::default();
        let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
            let wide = NodeRef::new();
            let fits = wide.natural_width().map(move |natural| natural <= room);
            forms(ui, &wide, &fits);
        });
        advance(&mut scene, &mut view, WINDOW_SIZE, 0)?;
        assert_eq!(view.tagged_bounds("row")?.width, shown);
    }
    Ok(())
}

#[test]
fn a_condition_reads_the_natural_width_of_a_sibling_with_a_weight() -> TestResult {
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), |ui| {
        let bar = NodeRef::new();
        let fits = bar.natural_width().map(|natural| natural <= 100.dp());
        ui.layout(Axis::Horizontal, Modifier::new().width(300.dp()), |ui| {
            let label = Modifier::new().shown_when(fits).tag("label");
            ui.spacer(40.dp(), 20.dp(), label);
            let weighted = Modifier::new().weight(1.0).node_ref(&bar).tag("bar");
            ui.spacer(80.dp(), 20.dp(), weighted);
        });
    });
    advance(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    assert_eq!(view.tagged_bounds("label")?.width, 40.dp());
    assert_eq!(
        view.tagged_bounds("bar")?,
        Rect::new(40.dp(), 0.dp(), 260.dp(), 20.dp())
    );
    Ok(())
}

#[test]
fn two_cards_take_the_width_of_the_wider_one() -> TestResult {
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), |ui| {
        let (a, b) = (NodeRef::new(), NodeRef::new());
        let wider = (a.natural_width().zip(&b.natural_width())).map(|(a, b)| a.max(b));
        ui.layout(Axis::Horizontal, Modifier::new(), |ui| {
            for (card, tag, content) in [(&a, "a", 120.dp()), (&b, "b", 80.dp())] {
                let sized = Modifier::new().node_ref(card).width(wider.clone());
                ui.layout(Axis::Vertical, sized.tag(tag), |ui| {
                    ui.spacer(content, 20.dp(), Modifier::new());
                });
            }
        });
    });
    advance(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    assert_eq!(view.tagged_bounds("a")?.width, 120.dp());
    assert_eq!(view.tagged_bounds("b")?.width, 120.dp());
    Ok(())
}

#[test]
fn an_action_key_shows_its_word_only_where_the_wide_form_fits() -> TestResult {
    for (room, shown) in [(139.6.dp(), 106.dp()), (85.6.dp(), 30.dp())] {
        let mut scene = Scene::default();
        let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
            let wide = NodeRef::new();
            let fits = wide.natural_width().map(move |natural| natural <= room);
            ui.layout(Axis::Horizontal, Modifier::new().width(room), |ui| {
                ui.layout(Axis::Horizontal, Modifier::new().tag("face"), |ui| {
                    let wide_form = Modifier::new().node_ref(&wide).shown_when(fits.clone());
                    ui.layout(Axis::Horizontal, wide_form.gap(6.dp()), |ui| {
                        ui.spacer(30.dp(), 30.dp(), Modifier::new());
                        ui.spacer(70.dp(), 20.dp(), Modifier::new());
                    });
                    let icon_alone = Modifier::new().shown_when(fits.map(|fits| !fits));
                    ui.spacer(30.dp(), 30.dp(), icon_alone);
                });
            });
        });
        advance(&mut scene, &mut view, WINDOW_SIZE, 0)?;
        assert_eq!(view.tagged_bounds("face")?.width, shown);
    }
    Ok(())
}

#[test]
fn the_natural_width_of_a_node_leaves_out_the_width_set_on_it() -> TestResult {
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), |ui| {
        let card = NodeRef::new();
        ui.layout(Axis::Vertical, Modifier::new(), |ui| {
            let padded = Modifier::new().padding(5.dp()).width(80.dp());
            ui.spacer(120.dp(), 20.dp(), padded.node_ref(&card).tag("card"));
            let natural_width = Modifier::new().width(card.natural_width());
            ui.spacer(Dp::ZERO, 10.dp(), natural_width.tag("reader"));
        });
    });
    advance(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    assert_eq!(view.tagged_bounds("card")?.width, 90.dp());
    assert_eq!(view.tagged_bounds("reader")?.width, 130.dp());
    Ok(())
}

#[test]
fn an_offset_reads_the_natural_width_of_a_node() -> TestResult {
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), |ui| {
        let card = NodeRef::new();
        ui.layout(Axis::Vertical, Modifier::new(), |ui| {
            let narrowed = Modifier::new().width(80.dp()).node_ref(&card);
            ui.spacer(120.dp(), 20.dp(), narrowed);
            let moved = Modifier::new().offset(card.natural_width(), Dp::ZERO);
            ui.spacer(10.dp(), 10.dp(), moved.tag("marker"));
        });
    });
    advance(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    assert_eq!(view.tagged_bounds("marker")?.x, 120.dp());
    Ok(())
}

#[test]
fn an_overlay_whose_filling_child_is_hidden_takes_no_share() -> TestResult {
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), |ui| {
        ui.layout(Axis::Horizontal, Modifier::new().width(300.dp()), |ui| {
            ui.layout(Arrangement::Overlay, Modifier::new().tag("overlay"), |ui| {
                let filling = Modifier::new().fill_max_width().shown_when(false);
                ui.spacer(Dp::ZERO, 20.dp(), filling);
                ui.spacer(40.dp(), 20.dp(), Modifier::new());
            });
            ui.spacer(Dp::ZERO, 20.dp(), Modifier::new().weight(1.0).tag("share"));
        });
    });
    advance(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    assert_eq!(view.tagged_bounds("overlay")?.width, 40.dp());
    assert_eq!(
        view.tagged_bounds("share")?,
        Rect::new(40.dp(), 0.dp(), 260.dp(), 20.dp())
    );
    Ok(())
}

#[test]
fn a_condition_follows_the_room_of_each_frame_without_composing_again() -> TestResult {
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), |ui| {
        let (room, wide) = (NodeRef::new(), NodeRef::new());
        let fits = (wide.natural_width().zip(&room.width()))
            .map(|(natural, available)| natural <= available);
        ui.layout(Axis::Vertical, Modifier::new(), |ui| {
            let filling = Modifier::new().fill_max_width().node_ref(&room);
            ui.spacer(Dp::ZERO, 10.dp(), filling);
            forms(ui, &wide, &fits);
        });
    });
    for (width, shown) in [
        (500.dp(), 120.dp()),
        (100.dp(), 40.dp()),
        (500.dp(), 120.dp()),
    ] {
        advance(&mut scene, &mut view, Size::new(width, 300.dp()), 0)?;
        assert_eq!(view.tagged_bounds("row")?.width, shown);
    }
    assert_eq!(view.stats().root_compositions, 1);
    Ok(())
}

#[test]
fn a_component_resolves_again_a_natural_width_that_changes_outside_it() -> TestResult {
    let title_width = State::new(120.dp());
    let composed_width = title_width.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let (title, composed_width) = (NodeRef::new(), composed_width.clone());
        let fits = title.natural_width().map(|natural| natural <= 150.dp());
        ui.layout(Axis::Vertical, Modifier::new(), |ui| {
            ui.scoped("title", Modifier::new(), move |ui| {
                let narrowed = Modifier::new().width(50.dp()).node_ref(&title);
                ui.spacer(composed_width.get(), 20.dp(), narrowed);
            });
            ui.scoped("forms", Modifier::new(), move |ui| {
                let (shown, hidden) = (fits.clone(), fits.map(|fits| !fits));
                ui.layout(Axis::Horizontal, Modifier::new().tag("row"), |ui| {
                    ui.spacer(30.dp(), 20.dp(), Modifier::new().shown_when(shown));
                    ui.spacer(10.dp(), 20.dp(), Modifier::new().shown_when(hidden));
                });
            });
        });
    });
    advance(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    assert_eq!(view.tagged_bounds("row")?.width, 30.dp());
    title_width.set(160.dp());
    advance(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    assert_eq!(view.tagged_bounds("row")?.width, 10.dp());
    Ok(())
}

#[test]
fn a_component_reads_a_natural_width_whose_probe_reads_inside_the_component() -> TestResult {
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), |ui| {
        let (label, mark) = (NodeRef::new(), NodeRef::new());
        let label_width = label.natural_width();
        ui.layout(Axis::Vertical, Modifier::new().width(300.dp()), |ui| {
            ui.layout(Axis::Horizontal, Modifier::new().node_ref(&label), |ui| {
                ui.spacer(Dp::ZERO, 10.dp(), Modifier::new().width(mark.width()));
            });
            ui.scoped("marked", Modifier::new(), move |ui| {
                ui.spacer(50.dp(), 10.dp(), Modifier::new().node_ref(&mark));
                let reader = Modifier::new().width(label_width.clone()).tag("reader");
                ui.spacer(Dp::ZERO, 10.dp(), reader);
            });
        });
    });
    for width in [500.dp(), 400.dp()] {
        advance(&mut scene, &mut view, Size::new(width, 300.dp()), 0)?;
        assert_eq!(view.tagged_bounds("reader")?.width, 50.dp());
    }
    Ok(())
}

#[test]
fn assistive_technology_and_the_tab_key_skip_a_hidden_button() -> TestResult {
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), |ui| {
        ui.layout(Axis::Vertical, Modifier::new(), |ui| {
            ui.text("shown", Modifier::new().on_click(|| {}).tag("shown"));
            ui.text("hidden", Modifier::new().on_click(|| {}).shown_when(false));
        });
    });
    advance(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    let names: Vec<_> = (view.accessibility().nodes.values())
        .filter_map(|node| node.name.clone())
        .collect();
    assert!(names.iter().any(|name| name == "shown"));
    assert!(!names.iter().any(|name| name == "hidden"));
    view.input(TAB);
    view.input(TAB);
    assert!(view.tagged_state("shown")?.focus.focused);
    Ok(())
}

#[test]
fn a_hidden_component_takes_no_shortcut() -> TestResult {
    let pressed = State::new(Vec::new());
    let counted = pressed.clone();
    let key = Shortcut::control(Key::Character('k'));
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        for (form, shown) in [("wide", true), ("narrow", false)] {
            let counted = counted.clone();
            ui.scoped(form, Modifier::new().shown_when(shown), move |ui| {
                let counted = counted.clone();
                ui.on_shortcut(key, move || counted.update(|pressed| pressed.push(form)));
            });
        }
    });
    advance(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    view.input(InputEvent::Shortcut(key));
    assert_eq!(pressed.get(), ["wide"]);
    Ok(())
}

#[test]
fn a_shortcut_composed_in_a_hidden_node_does_not_fire() -> TestResult {
    let pressed = State::new(Vec::new());
    let counted = pressed.clone();
    let key = Shortcut::control(Key::Character('k'));
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        for (form, shown) in [("wide", true), ("narrow", false)] {
            let counted = counted.clone();
            ui.layout(
                Arrangement::Overlay,
                Modifier::new().shown_when(shown),
                |ui| {
                    ui.on_shortcut(key, move || counted.update(|pressed| pressed.push(form)));
                },
            );
        }
    });
    advance(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    view.input(InputEvent::Shortcut(key));
    assert_eq!(pressed.get(), ["wide"]);
    Ok(())
}

#[test]
fn a_hidden_component_requests_no_frame_and_paints_the_latest_value_once_shown() -> TestResult {
    let (wide, narrow) = (Size::new(300.dp(), 100.dp()), Size::new(100.dp(), 100.dp()));
    let width = State::new(20.dp());
    let read = width.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let room = NodeRef::new();
        let fits = room.width().map(|available| available >= 200.dp());
        let read = read.clone();
        ui.layout(Axis::Vertical, Modifier::new(), |ui| {
            ui.scoped("panel", Modifier::new().shown_when(fits), move |ui| {
                let read = read.clone();
                ui.canvas(Size::new(50.dp(), 10.dp()), Modifier::new(), move |d| {
                    d.fill(Rect::new(0.dp(), 0.dp(), read.get(), 10.dp()), Color::WHITE);
                });
            });
            let filling = Modifier::new().fill_max_width().node_ref(&room);
            ui.spacer(Dp::ZERO, 10.dp(), filling);
        });
    });
    assert_eq!(
        fill_bounds(&mut scene, &mut view, wide)?,
        [Rect::new(0.dp(), 0.dp(), 20.dp(), 10.dp())]
    );
    assert!(fill_bounds(&mut scene, &mut view, narrow)?.is_empty());
    width.set(40.dp());
    assert!(fill_bounds(&mut scene, &mut view, narrow)?.is_empty());
    assert!(!view.needs_frame());
    assert_eq!(
        fill_bounds(&mut scene, &mut view, wide)?,
        [Rect::new(0.dp(), 0.dp(), 40.dp(), 10.dp())]
    );
    Ok(())
}
