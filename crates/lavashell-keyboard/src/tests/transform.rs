// SPDX-License-Identifier: GPL-3.0-only
use super::*;

type Points = [Point; 2];
type Find = Result<Points, TestError>;

/// A host that collects the transforms passed on by its keyboard, with the panel open and two
/// fingers down at the points found by `find` in its view.
fn pressed(find: impl Fn(&View) -> Find) -> Result<(Host, Points), TestError> {
    let host = Host::new(settings())?;
    host.pass_transforms();
    touched(host, find)
}

/// `host` with the panel open and two fingers down at the points found by `find` in its view.
fn touched(mut host: Host, find: impl Fn(&View) -> Find) -> Result<(Host, Points), TestError> {
    host.start(TextHints::default(), "")?;
    let [first, second] = find(&host.view)?;
    host.view.input(InputEvent::TouchDown(finger(1, first)));
    host.view.input(InputEvent::TouchDown(finger(2, second)));
    Ok((host, [first, second]))
}

fn on_keys(first: &'static str, second: &'static str) -> impl Fn(&View) -> Find {
    move |view| Ok([key(view, first)?, key(view, second)?])
}

/// Moves the second finger to each of `points` in turn.
fn slide(host: &mut Host, points: &[Point]) {
    for point in points {
        host.view.input(InputEvent::TouchMove(finger(2, *point)));
    }
}

/// Lifts both fingers from `points` and shows the frame that follows.
fn lift(host: &mut Host, points: Points) -> Result<(), TestError> {
    host.view.input(InputEvent::TouchUp(finger(1, points[0])));
    host.view.input(InputEvent::TouchUp(finger(2, points[1])));
    host.at(host.time + 500)
}

fn phases(host: &Host) -> Vec<DragPhase> {
    (host.transforms.borrow().iter())
        .map(|input| input.phase)
        .collect()
}

fn last_scale(host: &Host) -> Option<f64> {
    host.transforms.borrow().last().map(|input| input.scale)
}

#[test]
fn two_fingers_on_keys_that_come_together_pass_each_transform_to_the_host_and_type_nothing()
-> Result<(), TestError> {
    let (mut host, [first, second]) = pressed(on_keys("d", "k"))?;
    let started = Point::new(second.x - 20.dp(), second.y);
    let closer = Point::new((first.x + second.x) / 2.0, second.y);
    slide(&mut host, &[started, closer]);
    lift(&mut host, [first, closer])?;
    assert_eq!(
        phases(&host),
        [DragPhase::Started, DragPhase::Moved, DragPhase::Ended]
    );
    let expected = (closer.x - first.x) / (started.x - first.x);
    assert!(
        last_scale(&host).is_some_and(|scale| (scale - expected).abs() < 1e-9),
        "the scale is passed on unchanged: {:?}",
        last_scale(&host)
    );
    assert!(host.sent().is_empty());
    Ok(())
}

#[test]
fn two_fingers_on_keys_that_spread_pass_a_growing_scale_to_the_host_and_type_nothing()
-> Result<(), TestError> {
    let (mut host, [first, second]) = pressed(on_keys("f", "j"))?;
    let started = Point::new(second.x + 20.dp(), second.y);
    let apart = Point::new(second.x + 150.dp(), second.y);
    slide(&mut host, &[started, apart]);
    lift(&mut host, [first, apart])?;
    assert_eq!(phases(&host).last(), Some(&DragPhase::Ended));
    assert!(last_scale(&host).is_some_and(|scale| scale > 1.0));
    assert!(host.sent().is_empty());
    Ok(())
}

#[test]
fn two_fingers_on_the_bare_panel_pass_a_transform_to_the_host() -> Result<(), TestError> {
    let (mut host, [first, second]) = pressed(|view| {
        let panel = node(view, &Role::Group, "Keyboard")?.bounds;
        let y = panel.y + panel.height / 2.0;
        Ok([
            Point::new(panel.x + 2.dp(), y),
            Point::new(panel.x + panel.width - 2.dp(), y),
        ])
    })?;
    let started = Point::new(second.x - 20.dp(), second.y);
    let closer = Point::new((first.x + second.x) / 2.0, second.y);
    slide(&mut host, &[started, closer]);
    lift(&mut host, [first, closer])?;
    let phases = phases(&host);
    assert_eq!(phases.first(), Some(&DragPhase::Started), "{phases:?}");
    assert_eq!(phases.last(), Some(&DragPhase::Ended), "{phases:?}");
    assert!(last_scale(&host).is_some_and(|scale| scale < 1.0));
    assert!(host.sent().is_empty());
    Ok(())
}

#[test]
fn two_fingers_on_keys_that_stay_within_the_touch_slop_type_both_keys_and_pass_no_transform()
-> Result<(), TestError> {
    let (mut host, [first, second]) = pressed(on_keys("d", "k"))?;
    let nudged = Point::new(second.x + 4.dp(), second.y);
    slide(&mut host, &[nudged]);
    lift(&mut host, [first, nudged])?;
    assert!(host.transforms.borrow().is_empty());
    assert_eq!(host.sent(), [Host::commit("d"), Host::commit("k")]);
    Ok(())
}

#[test]
fn two_fingers_on_keys_that_come_together_type_both_keys_while_no_transform_action_is_set()
-> Result<(), TestError> {
    let (mut host, [first, second]) = touched(Host::new(settings())?, on_keys("d", "k"))?;
    let closer = Point::new(second.x - 20.dp(), second.y);
    slide(&mut host, &[closer]);
    lift(&mut host, [first, closer])?;
    assert_eq!(host.sent(), [Host::commit("d"), Host::commit("k")]);
    Ok(())
}
