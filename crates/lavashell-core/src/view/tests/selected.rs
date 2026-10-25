// SPDX-License-Identifier: GPL-3.0-only
use super::interaction::{SMALL_WINDOW_SIZE, tint};
use super::*;
use crate::animation::Tween;
use crate::float_asserts::assert_f64_eq;
use crate::style::{InteractionAppearance, InteractionState, StateStyles};
use std::time::Duration;

const SELECTED_TINT: f64 = 0.7;

fn selected_styles() -> StateStyles {
    StateStyles::new(|state| {
        let alpha = if state.selected { SELECTED_TINT } else { 0.0 };
        InteractionAppearance::default().tint(Color::WHITE.with_alpha(alpha), 0.dp())
    })
}

fn selected_view(
    scene: &mut Scene,
    selected: &State<bool>,
    enabled: &State<bool>,
) -> Result<(View, State<InteractionState>), TestError> {
    let (selected, enabled) = (selected.clone(), enabled.clone());
    let observed = State::new(InteractionState::default());
    let tracked = observed.clone();
    let styles = selected_styles();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        ui.spacer(
            100.dp(),
            40.dp(),
            Modifier::new()
                .tag("button")
                .accessible_name("Slot")
                .state_styles(styles.clone())
                .track_interaction(tracked.clone())
                .selected(selected.get())
                .enabled(enabled.get())
                .on_click(|| {}),
        );
    });
    advance(scene, &mut view, SMALL_WINDOW_SIZE, 0)?;
    Ok((view, observed))
}

fn exported<'a>(
    view: &'a View,
    name: &str,
) -> Result<&'a crate::accessibility::AccessibleNode, TestError> {
    view.accessibility()
        .nodes
        .values()
        .find(|node| node.name.as_deref() == Some(name))
        .required("an accessibility node with that name")
}

#[test]
fn selected_node_paints_its_selected_state_style() -> TestResult {
    let (selected, enabled) = (State::new(false), State::new(true));
    let mut scene = Scene::default();
    let (mut view, _) = selected_view(&mut scene, &selected, &enabled)?;
    assert_f64_eq(tint(&view, "button")?.alpha(), 0.0);
    selected.set(true);
    advance(&mut scene, &mut view, SMALL_WINDOW_SIZE, 0)?;
    assert_f64_eq(tint(&view, "button")?.alpha(), SELECTED_TINT);
    selected.set(false);
    advance(&mut scene, &mut view, SMALL_WINDOW_SIZE, 0)?;
    assert_f64_eq(tint(&view, "button")?.alpha(), 0.0);
    Ok(())
}

#[test]
fn selected_node_updates_its_interaction_observer() -> TestResult {
    let (selected, enabled) = (State::new(false), State::new(true));
    let mut scene = Scene::default();
    let (mut view, observed) = selected_view(&mut scene, &selected, &enabled)?;
    assert!(!observed.get().selected);
    selected.set(true);
    advance(&mut scene, &mut view, SMALL_WINDOW_SIZE, 0)?;
    assert!(observed.get().selected);
    selected.set(false);
    advance(&mut scene, &mut view, SMALL_WINDOW_SIZE, 0)?;
    assert!(!observed.get().selected);
    Ok(())
}

#[test]
fn selected_node_is_exported_as_selected_to_assistive_technology() -> TestResult {
    let (selected, enabled) = (State::new(false), State::new(true));
    let mut scene = Scene::default();
    let (mut view, _) = selected_view(&mut scene, &selected, &enabled)?;
    assert_eq!(exported(&view, "Slot")?.state.selected, Some(false));
    selected.set(true);
    advance(&mut scene, &mut view, SMALL_WINDOW_SIZE, 0)?;
    assert_eq!(exported(&view, "Slot")?.state.selected, Some(true));
    Ok(())
}

#[test]
fn selected_node_without_input_handlers_paints_its_selected_state_style() -> TestResult {
    let styles = selected_styles();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        ui.spacer(
            100.dp(),
            40.dp(),
            Modifier::new()
                .tag("button")
                .state_styles(styles.clone())
                .selected(true),
        );
    });
    advance(&mut scene, &mut view, SMALL_WINDOW_SIZE, 0)?;
    assert_f64_eq(tint(&view, "button")?.alpha(), SELECTED_TINT);
    Ok(())
}

fn tweened_selected_styles() -> StateStyles {
    selected_styles().motion(Tween::new(Duration::from_millis(100)).preserve_velocity(false))
}

fn unhandled_view(selected: &State<bool>) -> View {
    let selected = selected.clone();
    let styles = tweened_selected_styles();
    View::new(crate::platform::fixture::facts(), move |ui| {
        ui.spacer(
            100.dp(),
            40.dp(),
            Modifier::new()
                .tag("button")
                .state_styles(styles.clone())
                .selected(selected.get()),
        );
    })
}

fn view_declaring_selection_while_selected(selected: &State<bool>) -> View {
    let selected = selected.clone();
    let styles = tweened_selected_styles();
    View::new(crate::platform::fixture::facts(), move |ui| {
        let modifier = Modifier::new().tag("button").state_styles(styles.clone());
        let modifier = if selected.get() {
            modifier.selected(true)
        } else {
            modifier
        };
        ui.spacer(100.dp(), 40.dp(), modifier);
    })
}

fn view_styled_while_selected(selected: &State<bool>) -> View {
    let selected = selected.clone();
    let styles = tweened_selected_styles();
    View::new(crate::platform::fixture::facts(), move |ui| {
        let modifier = Modifier::new().tag("button");
        let modifier = if selected.get() {
            modifier.state_styles(styles.clone()).selected(true)
        } else {
            modifier
        };
        ui.spacer(100.dp(), 40.dp(), modifier);
    })
}

fn assert_halfway_to_selected(view: &View) -> TestResult {
    assert!((tint(view, "button")?.alpha() - SELECTED_TINT / 2.0).abs() < 0.01);
    Ok(())
}

fn paints_tint(scene: &Scene) -> bool {
    (scene.commands().iter()).any(|command| matches!(command, DrawCommand::Fill { .. }))
}

#[test]
fn a_node_without_input_handlers_fades_its_selected_style_in() -> TestResult {
    let selected = State::new(false);
    let mut scene = Scene::default();
    let mut view = unhandled_view(&selected);
    advance(&mut scene, &mut view, SMALL_WINDOW_SIZE, 0)?;
    selected.set(true);
    advance(&mut scene, &mut view, SMALL_WINDOW_SIZE, 0)?;
    advance(&mut scene, &mut view, SMALL_WINDOW_SIZE, 50)?;
    assert_halfway_to_selected(&view)?;
    Ok(())
}

#[test]
fn a_node_without_input_handlers_fades_its_selected_style_out() -> TestResult {
    let selected = State::new(true);
    let mut scene = Scene::default();
    let mut view = unhandled_view(&selected);
    advance(&mut scene, &mut view, SMALL_WINDOW_SIZE, 0)?;
    advance(&mut scene, &mut view, SMALL_WINDOW_SIZE, 100)?;
    selected.set(false);
    advance(&mut scene, &mut view, SMALL_WINDOW_SIZE, 100)?;
    advance(&mut scene, &mut view, SMALL_WINDOW_SIZE, 150)?;
    assert_halfway_to_selected(&view)?;
    Ok(())
}

#[test]
fn a_node_without_input_handlers_paints_nothing_once_its_selected_style_has_faded_out() -> TestResult
{
    let selected = State::new(true);
    let mut view = unhandled_view(&selected);
    let mut scene = Scene::default();
    show_frame(&mut scene, &mut view, SMALL_WINDOW_SIZE, 0)?;
    show_frame(&mut scene, &mut view, SMALL_WINDOW_SIZE, 100)?;
    assert!(paints_tint(&scene));
    selected.set(false);
    show_frame(&mut scene, &mut view, SMALL_WINDOW_SIZE, 100)?;
    show_frame(&mut scene, &mut view, SMALL_WINDOW_SIZE, 200)?;
    assert!(!paints_tint(&scene));
    Ok(())
}

#[test]
fn a_node_that_declares_a_selection_again_fades_its_selected_style_in_again() -> TestResult {
    let selected = State::new(true);
    let mut scene = Scene::default();
    let mut view = view_declaring_selection_while_selected(&selected);
    advance(&mut scene, &mut view, SMALL_WINDOW_SIZE, 0)?;
    advance(&mut scene, &mut view, SMALL_WINDOW_SIZE, 100)?;
    selected.set(false);
    advance(&mut scene, &mut view, SMALL_WINDOW_SIZE, 100)?;
    selected.set(true);
    advance(&mut scene, &mut view, SMALL_WINDOW_SIZE, 200)?;
    advance(&mut scene, &mut view, SMALL_WINDOW_SIZE, 250)?;
    assert_halfway_to_selected(&view)?;
    Ok(())
}

#[test]
fn a_node_that_gets_its_state_styles_again_fades_its_selected_style_in_again() -> TestResult {
    let selected = State::new(true);
    let mut scene = Scene::default();
    let mut view = view_styled_while_selected(&selected);
    advance(&mut scene, &mut view, SMALL_WINDOW_SIZE, 0)?;
    advance(&mut scene, &mut view, SMALL_WINDOW_SIZE, 100)?;
    selected.set(false);
    advance(&mut scene, &mut view, SMALL_WINDOW_SIZE, 100)?;
    selected.set(true);
    advance(&mut scene, &mut view, SMALL_WINDOW_SIZE, 200)?;
    advance(&mut scene, &mut view, SMALL_WINDOW_SIZE, 250)?;
    assert_halfway_to_selected(&view)?;
    Ok(())
}

#[test]
fn a_node_that_is_selected_when_it_appears_fades_its_selected_style_in() -> TestResult {
    let selected = State::new(true);
    let mut scene = Scene::default();
    let mut view = unhandled_view(&selected);
    advance(&mut scene, &mut view, SMALL_WINDOW_SIZE, 0)?;
    advance(&mut scene, &mut view, SMALL_WINDOW_SIZE, 50)?;
    assert_halfway_to_selected(&view)?;
    Ok(())
}

#[test]
fn selected_node_without_other_metadata_is_exported() -> TestResult {
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), |ui| {
        ui.spacer(100.dp(), 40.dp(), Modifier::new().selected(true));
    });
    advance(&mut scene, &mut view, SMALL_WINDOW_SIZE, 0)?;
    assert!(
        view.accessibility()
            .nodes
            .values()
            .any(|node| node.state.selected == Some(true))
    );
    Ok(())
}

#[test]
fn selected_state_survives_disabling_and_loss_of_window_focus() -> TestResult {
    let (selected, enabled) = (State::new(true), State::new(true));
    let mut scene = Scene::default();
    let (mut view, _) = selected_view(&mut scene, &selected, &enabled)?;
    view.input(InputEvent::Move(Point::new(20.dp(), 20.dp())));
    view.input(InputEvent::WindowFocus(false));
    enabled.set(false);
    advance(&mut scene, &mut view, SMALL_WINDOW_SIZE, 0)?;
    let state = view.tagged_state("button")?;
    assert!(state.disabled);
    assert!(state.selected);
    Ok(())
}
