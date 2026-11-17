// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::accessibility::{AccessibilityAction, ActionOutcome, ActionRequest};
use crate::geometry::Axis;
use std::cell::RefCell;
use std::rc::Rc;

type Calls = Rc<RefCell<Vec<&'static str>>>;

fn handlers(scene: &mut Scene, calls: &Calls) -> Result<View, TestError> {
    let calls = calls.clone();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let calls = calls.clone();
        ui.layout(Axis::Vertical, Modifier::new(), move |ui| {
            let call = |name| {
                let calls = calls.clone();
                move || calls.borrow_mut().push(name)
            };
            let (context, activate) = (call("context"), call("activate"));
            let twice = Modifier::new()
                .tag("twice")
                .on_click(call("first click"))
                .on_click(call("second click"))
                .on_context(move |_| context());
            ui.spacer(100.dp(), 40.dp(), twice);
            let both = Modifier::new()
                .tag("both")
                .on_activate(move |_| activate())
                .on_click(call("click"));
            ui.spacer(100.dp(), 40.dp(), both);
        });
    });
    advance(scene, &mut view, WINDOW_SIZE, 0)?;
    Ok(view)
}

fn tap(view: &mut View, at: Point) {
    view.input(InputEvent::Press(at));
    view.input(InputEvent::Release(at));
}

#[test]
fn a_second_handler_of_one_kind_replaces_the_first_and_context_reaches_its_handler() -> TestResult {
    let (mut scene, calls) = (Scene::default(), Calls::default());
    let mut view = handlers(&mut scene, &calls)?;
    let twice = view.tagged_bounds(&"twice")?.center();
    tap(&mut view, twice);
    view.input(InputEvent::Context(twice));
    assert_eq!(*calls.borrow(), ["second click", "context"]);
    Ok(())
}

#[test]
fn activate_runs_before_click_from_a_tap_a_key_and_an_accessibility_request() -> TestResult {
    let (mut scene, calls) = (Scene::default(), Calls::default());
    let mut view = handlers(&mut scene, &calls)?;
    let both = view.tagged_bounds(&"both")?.center();
    tap(&mut view, both);
    view.input(InputEvent::Key {
        key: Key::Space,
        shift: false,
    });
    view.input(InputEvent::KeyRelease(Key::Space));
    let target = (view.nodes.iter())
        .find(|node| node.has_tag(&"both"))
        .required("a node tagged both")?
        .id;
    let request = ActionRequest {
        target,
        action: AccessibilityAction::Activate,
    };
    assert_eq!(
        view.accessibility_action(request),
        Ok(ActionOutcome::Performed)
    );
    assert_eq!(
        *calls.borrow(),
        [
            "activate", "click", "activate", "click", "activate", "click"
        ]
    );
    Ok(())
}
