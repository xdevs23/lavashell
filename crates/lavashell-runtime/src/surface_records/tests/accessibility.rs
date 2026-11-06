// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::{AccessibilityFailure, InputRouter, RoutedInput};
use accesskit::{Action, Node, NodeId as NativeId, Role as NativeRole, TreeId, TreeUpdate};
use lavashell_core::accessibility::{
    AccessibilityAction, AccessibleValue, ActionError, ActionKind, Role, Semantics,
};
use lavashell_core::reactive::State;
use lavashell_core::units::NumberError;
use lavashell_core::view::InputEvent;
use lavashell_platform::{AccessibilityEvent, AccessibilityRequest, DeviceEvent, KeyboardDevice};
use lavashell_test_fixtures::FrameFailure;
use std::cell::Cell;

const KEYBOARD: KeyboardDevice = KeyboardDevice(1);

/// Opens a surface named "Window", which the host configured and whose view shows a button with the name held by
/// `name`. `activate` performs each activation of the button.
fn open_button(
    records: &mut SurfaceRecords,
    name: &State<String>,
    activate: impl FnMut(AccessibilityAction) -> Result<(), ActionError> + Clone + 'static,
) -> Result<SurfaceId, TestError> {
    let name = name.clone();
    let view = View::new(lavashell_test_fixtures::facts(), move |ui| {
        let button = Semantics {
            role: Some(Role::Button),
            name: Some(name.get()),
            ..Semantics::default()
        };
        let modifier = Modifier::new()
            .semantics(button)
            .on_accessibility_action(ActionKind::Activate, activate.clone());
        ui.spacer(100.dp(), 20.dp(), modifier);
    });
    open_configured(records, view)
}

/// Opens a surface, which the host configured, whose view shows a progress indicator with the value held by
/// `progress`. The value changes the semantics of the view and leaves its drawing unchanged.
fn open_progress(
    records: &mut SurfaceRecords,
    progress: &State<f64>,
) -> Result<SurfaceId, TestError> {
    let progress = progress.clone();
    let view = View::new(lavashell_test_fixtures::facts(), move |ui| {
        let progress = progress.clone();
        let indicator = Semantics {
            role: Some(Role::ProgressIndicator),
            name: Some("Progress".into()),
            ..Semantics::default()
        };
        let modifier = Modifier::new()
            .semantics(indicator)
            .semantics_dynamic(move |semantics| {
                semantics.value = Some(AccessibleValue::Number(progress.get()));
            });
        ui.spacer(100.dp(), 20.dp(), modifier);
    });
    open_configured(records, view)
}

fn open_configured(records: &mut SurfaceRecords, view: View) -> Result<SurfaceId, TestError> {
    let size = Size::new(100.dp(), 20.dp());
    let surface = records.open(view, size, Density::ONE, None, "Window".into())?;
    assert_eq!(records.handle(surface, configure(None)), None);
    Ok(surface)
}

fn start_name() -> State<String> {
    State::new("Start".into())
}

/// Frames each view that the runtime shows, and counts the frames in `shown`.
fn show(shown: &Cell<u32>) -> impl FnMut(&mut View, Viewport) -> Result<(), FrameFailure> {
    move |view: &mut View, viewport: Viewport| {
        let mut scene = RecordedScene::default();
        (view.frame_into(viewport, &mut NoText, &mut scene)).without_failures()?;
        shown.set(shown.get() + 1);
        Ok(())
    }
}

fn fail_to_show(_: &mut View, _: Viewport) -> Result<(), TestError> {
    Err(TestError::NotShown)
}

/// The tree that `request` publishes for `surface`.
fn published_tree(
    request: Option<AccessibilityRequest>,
    surface: SurfaceId,
) -> Result<Box<TreeUpdate>, TestError> {
    match request {
        Some(AccessibilityRequest::Publish {
            surface: published,
            tree,
        }) if published == surface => Ok(tree),
        _ => Err(TestError::NotPublished),
    }
}

/// The tree that the only request of `requests` publishes for `surface`.
fn only_published_tree(
    requests: Vec<AccessibilityRequest>,
    surface: SurfaceId,
) -> Result<Box<TreeUpdate>, TestError> {
    assert_eq!(requests.len(), 1);
    published_tree(requests.into_iter().next(), surface)
}

fn button(tree: &TreeUpdate) -> Result<(NativeId, &Node), TestError> {
    (tree.nodes.iter())
        .find(|(_, node)| node.role() == NativeRole::Button)
        .map(|(id, node)| (*id, node))
        .ok_or(TestError::NoButton)
}

/// Activates the accessibility of `surface` and returns the tree that the activation publishes.
fn activate_accessibility(
    records: &mut SurfaceRecords,
    surface: SurfaceId,
) -> Result<Box<TreeUpdate>, TestError> {
    let activated = AccessibilityEvent::Activate(surface);
    let outcome = records.handle_accessibility(activated, show(&Cell::new(0)));
    assert!(outcome.failures.is_empty());
    published_tree(outcome.publish, surface)
}

/// The request of an assistive technology to click `node` of `surface`.
fn click(surface: SurfaceId, node: NativeId) -> AccessibilityEvent {
    let request = accesskit::ActionRequest {
        action: Action::Click,
        target_tree: TreeId::ROOT,
        target_node: node,
        data: None,
    };
    AccessibilityEvent::Action(surface, request)
}

#[test]
fn an_activation_shows_the_first_frame_and_publishes_the_whole_tree() -> TestResult {
    let mut records = SurfaceRecords::new(PreferenceOverrides::default());
    let surface = open_button(&mut records, &start_name(), |_| Ok(()))?;
    let shown = Cell::new(0);
    let activated = AccessibilityEvent::Activate(surface);
    let outcome = records.handle_accessibility(activated, show(&shown));
    assert_eq!(shown.get(), 1);
    assert!(outcome.failures.is_empty());
    let tree = published_tree(outcome.publish, surface)?;
    assert_eq!(button(&tree)?.1.label(), Some("Start"));
    Ok(())
}

#[test]
fn an_action_runs_the_handler_of_the_app_and_publishes_the_semantics_it_changed() -> TestResult {
    let mut records = SurfaceRecords::new(PreferenceOverrides::default());
    let name = start_name();
    let renamed = name.clone();
    let surface = open_button(&mut records, &name, move |_| {
        renamed.set("Stop".into());
        Ok(())
    })?;
    let first = activate_accessibility(&mut records, surface)?;
    let (button_id, _) = button(&first)?;
    let outcome = records.handle_accessibility(click(surface, button_id), show(&Cell::new(0)));
    assert!(outcome.failures.is_empty());
    let tree = published_tree(outcome.publish, surface)?;
    assert_eq!(button(&tree)?.1.label(), Some("Stop"));
    Ok(())
}

#[test]
fn a_failed_action_publishes_the_whole_tree_and_returns_the_failure() -> TestResult {
    let mut records = SurfaceRecords::new(PreferenceOverrides::default());
    let surface = open_button(&mut records, &start_name(), |_| {
        Err(ActionError::Count(NumberError::NotFinite))
    })?;
    let first = activate_accessibility(&mut records, surface)?;
    let (button_id, _) = button(&first)?;
    let outcome = records.handle_accessibility(click(surface, button_id), show(&Cell::new(0)));
    assert!(matches!(
        outcome.failures.as_slice(),
        [AccessibilityFailure::PerformAction(ActionError::Count(_))]
    ));
    let _tree = published_tree(outcome.publish, surface)?;
    Ok(())
}

#[test]
fn a_frame_that_fails_to_show_is_returned_next_to_the_whole_tree() -> TestResult {
    let mut records = SurfaceRecords::new(PreferenceOverrides::default());
    let surface = open_button(&mut records, &start_name(), |_| Ok(()))?;
    let _first = activate_accessibility(&mut records, surface)?;
    let outcome = records.handle_accessibility(AccessibilityEvent::Activate(surface), fail_to_show);
    assert!(matches!(
        outcome.failures.as_slice(),
        [AccessibilityFailure::Show(TestError::NotShown)]
    ));
    let _tree = published_tree(outcome.publish, surface)?;
    Ok(())
}

#[test]
fn a_deactivated_surface_runs_no_action_and_publishes_nothing() -> TestResult {
    let mut records = SurfaceRecords::new(PreferenceOverrides::default());
    let presses = Rc::new(Cell::new(0));
    let counter = presses.clone();
    let surface = open_button(&mut records, &start_name(), move |_| {
        counter.set(counter.get() + 1);
        Ok(())
    })?;
    let first = activate_accessibility(&mut records, surface)?;
    let (button_id, _) = button(&first)?;
    let deactivated = AccessibilityEvent::Deactivate(surface);
    let outcome = records.handle_accessibility(deactivated, show(&Cell::new(0)));
    assert!(outcome.publish.is_none() && outcome.failures.is_empty());
    let outcome = records.handle_accessibility(click(surface, button_id), show(&Cell::new(0)));
    assert!(outcome.publish.is_none() && outcome.failures.is_empty());
    assert_eq!(presses.get(), 0);
    Ok(())
}

#[test]
fn a_surface_that_the_host_has_not_configured_shows_and_publishes_nothing() -> TestResult {
    let mut records = SurfaceRecords::new(PreferenceOverrides::default());
    let surface = open(&mut records)?;
    let shown = Cell::new(0);
    let activated = AccessibilityEvent::Activate(surface);
    let outcome = records.handle_accessibility(activated, show(&shown));
    assert_eq!(shown.get(), 0);
    assert!(outcome.publish.is_none() && outcome.failures.is_empty());
    assert!(records.update_accessibility().is_empty());
    Ok(())
}

#[test]
fn semantics_that_change_without_a_frame_are_published_once() -> TestResult {
    let mut records = SurfaceRecords::new(PreferenceOverrides::default());
    let progress = State::new(1.0);
    let surface = open_progress(&mut records, &progress)?;
    let _first = activate_accessibility(&mut records, surface)?;
    frame(&mut records, surface)?;
    progress.set(2.0);
    let tree = only_published_tree(records.update_accessibility(), surface)?;
    let value = (tree.nodes.iter()).find_map(|(_, node)| node.numeric_value());
    assert_eq!(value, Some(2.0));
    assert!(records.update_accessibility().is_empty());
    Ok(())
}

#[test]
fn a_frame_publishes_the_semantics_it_changed_once() -> TestResult {
    let mut records = SurfaceRecords::new(PreferenceOverrides::default());
    let name = start_name();
    let surface = open_button(&mut records, &name, |_| Ok(()))?;
    let _first = activate_accessibility(&mut records, surface)?;
    name.set("Stop".into());
    frame(&mut records, surface)?;
    let tree = published_tree(records.publish_accessibility(surface), surface)?;
    assert_eq!(button(&tree)?.1.label(), Some("Stop"));
    assert_eq!(records.publish_accessibility(surface), None);
    Ok(())
}

#[test]
fn a_retired_surface_publishes_nothing() -> TestResult {
    let mut records = SurfaceRecords::new(PreferenceOverrides::default());
    let name = start_name();
    let surface = open_button(&mut records, &name, |_| Ok(()))?;
    let _first = activate_accessibility(&mut records, surface)?;
    assert!(records.handle(surface, SurfaceEvent::Closed).is_some());
    records.retire(surface);
    name.set("Stop".into());
    assert_eq!(records.publish_accessibility(surface), None);
    assert!(records.update_accessibility().is_empty());
    Ok(())
}

#[test]
fn the_window_focus_of_each_open_surface_follows_the_keyboards() -> TestResult {
    let mut records = SurfaceRecords::new(PreferenceOverrides::default());
    let window = open(&mut records)?;
    let panel = open(&mut records)?;
    let mut input = InputRouter::new(None);
    let focus_on = |surface| DeviceEvent::KeyboardFocus {
        keyboard: KEYBOARD,
        surface,
    };
    let routed_focus = |surface, focused| RoutedInput::Input {
        surface,
        input: InputEvent::WindowFocus(focused),
        pointer: None,
    };
    let window_focus = |surface, focused| AccessibilityRequest::SetWindowFocus { surface, focused };
    assert_eq!(
        input.route(focus_on(Some(window))),
        [routed_focus(window, true)]
    );
    assert_eq!(
        records.window_focus_requests(|surface| input.is_keyboard_focused(surface)),
        [window_focus(window, true), window_focus(panel, false)]
    );
    assert!(records.handle(window, SurfaceEvent::Closed).is_some());
    assert_eq!(
        input.route(focus_on(Some(panel))),
        [routed_focus(window, false), routed_focus(panel, true)]
    );
    assert_eq!(
        records.window_focus_requests(|surface| input.is_keyboard_focused(surface)),
        [window_focus(panel, true)]
    );
    Ok(())
}
