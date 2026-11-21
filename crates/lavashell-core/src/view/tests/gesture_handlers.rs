// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::accessibility::{AccessibilityAction, ActionOutcome, ActionRequest};
use crate::geometry::Axis;
use std::cell::RefCell;
use std::rc::Rc;

type Calls = Rc<RefCell<Vec<&'static str>>>;

fn record<T>(calls: &Calls, name: &'static str) -> impl FnMut(T) + 'static {
    let calls = calls.clone();
    move |_| calls.borrow_mut().push(name)
}

fn handlers(scene: &mut Scene, calls: &Calls) -> Result<View, TestError> {
    let calls = calls.clone();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let calls = calls.clone();
        ui.layout(Axis::Vertical, Modifier::new(), move |ui| {
            let twice = Modifier::new()
                .tag("twice")
                .on_activate(record(&calls, "first activate"))
                .on_activate(record(&calls, "second activate"))
                .on_context(record(&calls, "context"));
            ui.spacer(100.dp(), 40.dp(), twice);
            let activated = Modifier::new()
                .tag("activated")
                .on_activate(record(&calls, "activate"));
            ui.spacer(100.dp(), 40.dp(), activated);
        });
    });
    advance(scene, &mut view, WINDOW_SIZE, 0)?;
    Ok(view)
}

#[test]
fn a_second_handler_of_one_kind_replaces_the_first_and_context_reaches_its_handler() -> TestResult {
    let (mut scene, calls) = (Scene::default(), Calls::default());
    let mut view = handlers(&mut scene, &calls)?;
    let twice = view.tagged_center(&"twice")?;
    press_and_release_at(&mut view, twice);
    view.input(InputEvent::Context(twice));
    assert_eq!(*calls.borrow(), ["second activate", "context"]);
    Ok(())
}

#[test]
fn activate_runs_from_a_press_a_key_and_an_accessibility_request() -> TestResult {
    let (mut scene, calls) = (Scene::default(), Calls::default());
    let mut view = handlers(&mut scene, &calls)?;
    let activated = view.tagged_center(&"activated")?;
    press_and_release_at(&mut view, activated);
    view.input(InputEvent::Key {
        key: Key::Space,
        shift: false,
    });
    view.input(InputEvent::KeyRelease(Key::Space));
    let target = (view.nodes.iter())
        .find(|node| node.has_tag(&"activated"))
        .required("a node tagged activated")?
        .id;
    let request = ActionRequest {
        target,
        action: AccessibilityAction::Activate,
    };
    assert_eq!(
        view.accessibility_action(request),
        Ok(ActionOutcome::Performed)
    );
    assert_eq!(*calls.borrow(), ["activate", "activate", "activate"]);
    Ok(())
}
