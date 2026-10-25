// SPDX-License-Identifier: GPL-3.0-only
use super::material_motion::painted_fills;
use super::*;
use crate::animation::{
    Animatable, Animation, Easing, Keyframe, Keyframes, MotionSpec, Playback, Repeat, Tween,
};
use crate::float_asserts::assert_f64s_eq;
use crate::scene::tree::{Change, Node, PresenceVectors, Slot, SlotChange, SlotValue};
use crate::scroll::ScrollState;
use crate::units::ScaleFactor;
use std::time::Duration;

fn linear(ms: u64) -> Tween {
    Tween::new(Duration::from_millis(ms))
        .easing(Easing::Linear)
        .preserve_velocity(false)
}

fn plays<T: SlotValue>(slot: &Slot<T>) -> bool {
    matches!(slot, Slot::Motion(_))
}

fn motions(update: &SceneUpdate) -> usize {
    (update.changes.iter())
        .map(|change| match change {
            Change::Insert {
                node: Node::Group(group),
                ..
            } => usize::from(plays(&group.transform)) + usize::from(plays(&group.opacity)),
            Change::SetSlot {
                slot: SlotChange::Transform(slot),
                ..
            } => usize::from(plays(slot)),
            Change::SetSlot {
                slot: SlotChange::Opacity(slot),
                ..
            } => usize::from(plays(slot)),
            _ => 0,
        })
        .sum()
}

#[test]
fn the_parts_of_the_vector_of_a_visual_are_the_vectors_of_its_parts() {
    use crate::color::ColorInterpolation;
    use crate::units::Fraction;
    use crate::view::transition::{Visual, VisualAppearance, VisualVectors};
    let visual = Visual {
        rect: Rect::new(1.dp(), 2.dp(), 3.dp(), 4.dp()),
        presence: (Presence::default())
            .opacity(crate::constant!(Fraction::try_new(0.25)))
            .scale(crate::constant!(ScaleFactor::try_new(2.0)))
            .slide(crate::constant!(slide_offset(5.0, 6.0))),
        appearance: VisualAppearance {
            background: Color::WHITE.interpolated(ColorInterpolation::default()),
            corners: Corners::all(7.dp()),
            opacity: 0.5,
            material: None,
            group_material: None,
        },
    };
    let vector = visual.vector();
    let parts = VisualVectors::of(&vector);
    assert_f64s_eq(&parts.rect.0, &[1.0, 2.0, 3.0, 4.0]);
    let presence = PresenceVectors::of(parts.presence);
    assert_f64s_eq(&presence.opacity.0, &[0.25]);
    assert_f64s_eq(&presence.scale.0, &[2.0_f64.ln()]);
    assert_f64s_eq(&presence.offset.0, &[5.0, 6.0]);
    assert_eq!(*parts.appearance, visual.appearance.vector());
}

/// A view with a white card of 100 by 60 dp that counts its taps in `taps`. The card is in a scoped
/// component keyed "card" that slides in over a second from 200 dp to the right.
fn sliding_card(taps: &State<u32>) -> View {
    let taps = taps.clone();
    View::new(crate::platform::fixture::facts(), move |ui| {
        let taps = taps.clone();
        let slide = Presence::default().slide(crate::constant!(slide_offset(200.0, 0.0)));
        ui.scoped(
            "card",
            Modifier::new().transition(Transition::new(linear(1000)).enter(slide)),
            move |ui| {
                let taps = taps.clone();
                ui.spacer(
                    100.dp(),
                    60.dp(),
                    Modifier::new()
                        .background(Color::WHITE)
                        .on_click(move || taps.update(|count| *count += 1)),
                );
            },
        );
    })
}

#[test]
fn a_sliding_card_emits_one_motion_and_repaints_nothing_until_it_ends() -> TestResult {
    let mut view = sliding_card(&State::new(0));
    let mut scene = Scene::default();
    let update = frame(&mut view, WINDOW_SIZE, 0)?;
    assert_eq!(motions(&update), 1);
    scene.show(update)?;
    assert!(!view.needs_frame());
    assert_eq!(view.next_deadline(), Some(Duration::from_secs(1)));
    let painted = view.stats().painted_components;
    for ms in [250, 500, 999] {
        assert!(frame(&mut view, WINDOW_SIZE, ms)?.changes.is_empty());
    }
    assert_eq!(view.stats().painted_components, painted);
    let card = |scene: &Scene| painted_fills(&scene.commands()).map(|fills| fills[0].rect.x);
    assert_eq!(card(scene.at(500))?, 100.dp());
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 1000)?;
    assert_eq!(card(&scene)?, 0.dp());
    assert_eq!(view.next_deadline(), None);
    Ok(())
}

#[test]
fn a_tap_during_the_slide_hits_the_card_where_it_was_shown() -> TestResult {
    let taps = State::new(0);
    let mut scene = Scene::default();
    let mut view = sliding_card(&taps);
    advance(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    view.record_presentation(Duration::from_millis(500));
    tap_at(&mut view, Point::new(250.dp(), 30.dp()));
    assert_eq!(taps.get(), 0);
    tap_at(&mut view, Point::new(150.dp(), 30.dp()));
    assert_eq!(taps.get(), 1);
    Ok(())
}

#[test]
fn a_node_removed_during_a_move_moves_on_while_it_fades() -> TestResult {
    const PIECE: Color = Color::rgb(0x0040_a0ff);
    let (moved, shown) = (State::new(false), State::new(true));
    let (target, present) = (moved.clone(), shown.clone());
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        if present.get() {
            let x = if target.get() { 200.dp() } else { 0.dp() };
            ui.key(
                "slot",
                Modifier::new().transition(Transition::new(linear(1000)).exit(Presence::fade())),
                |ui| {
                    ui.key(
                        "piece",
                        Modifier::new()
                            .offset(x, 0.dp())
                            .background(PIECE)
                            .transition(Transition::new(linear(1000)).reflow()),
                        |ui| {
                            ui.spacer(20.dp(), 20.dp(), Modifier::new());
                        },
                    );
                },
            );
        }
    });
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    moved.set(true);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    shown.set(false);
    // The frame at 500 ms, composed without the node keyed "slot", starts the fade of that node as
    // the only motion in its update. The move of "piece" goes on in the scene from the motion sent
    // by the earlier frame.
    let update = frame(&mut view, WINDOW_SIZE, 500)?;
    assert_eq!(motions(&update), 1);
    scene.show(update)?;
    assert!(!view.needs_frame());
    assert_eq!(view.next_deadline(), Some(Duration::from_secs(1)));
    let piece = |scene: &Scene| painted_fills(&scene.commands()).map(|fills| fills[0].rect.x);
    let fade = |scene: &Scene| scene.groups()[0].opacity;
    let scene_at_750 = scene.at(750);
    assert_eq!((piece(scene_at_750)?, fade(scene_at_750)), (150.dp(), 0.75));
    let scene_at_1000 = scene.at(1000);
    assert_eq!(
        (piece(scene_at_1000)?, fade(scene_at_1000)),
        (200.dp(), 0.5)
    );
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 1500)?;
    assert!(scene.commands().is_empty());
    Ok(())
}

#[test]
fn an_endless_pulse_plays_in_the_scene_without_frames_of_the_view() -> TestResult {
    let pulse = Animation::new(1.0_f64)?;
    let pulsing = pulse.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        ui.spacer(
            20.dp(),
            20.dp(),
            Modifier::new().animated_opacity(pulsing.clone()),
        );
    });
    advance(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    let frames = Keyframes::new([
        Keyframe::new(Duration::ZERO, 1.0),
        Keyframe::new(Duration::from_millis(400), 0.2),
    ])?;
    let forever = Playback {
        repeat: Repeat::Forever,
        ..Playback::default()
    };
    pulse.play(frames, forever);
    assert!(view.needs_frame());
    let update = frame(&mut view, WINDOW_SIZE, 0)?;
    assert_eq!(motions(&update), 1);
    assert!(!view.needs_frame());
    assert_eq!(view.next_deadline(), None);
    Ok(())
}

#[test]
fn jumping_a_smooth_scroll_to_its_offset_repaints_nothing() -> TestResult {
    let scroll = ScrollState::new(Axis::Vertical);
    let list = scroll.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let list = list.clone();
        ui.scoped("list", Modifier::new(), move |ui| {
            let spring = crate::spring!(Duration::from_millis(300), 1.0);
            if let Err(error) = list.smooth(ui.motion(), spring) {
                ui.fail(error);
            }
            ui.scroll(
                list.clone(),
                Modifier::new().size(100.dp(), 100.dp()),
                |ui| {
                    ui.spacer(100.dp(), 400.dp(), Modifier::new().background(Color::WHITE));
                },
            );
        });
    });
    advance(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    scroll.jump_to(40.dp());
    advance(&mut scene, &mut view, WINDOW_SIZE, 10)?;
    advance(&mut scene, &mut view, WINDOW_SIZE, 15)?;
    let painted = view.stats().painted_components;
    for ms in [20, 30, 40] {
        scroll.jump_to(40.dp());
        assert!(!view.needs_frame());
        advance(&mut scene, &mut view, WINDOW_SIZE, ms)?;
    }
    assert_eq!(view.stats().painted_components, painted);
    Ok(())
}

/// A view with a list of 100 by 100 dp that scrolls `scroll` with the motion `spec`. The list
/// contains ten rows 40 dp high, each in a scoped component.
fn smooth_rows(scroll: &ScrollState, spec: MotionSpec) -> View {
    let list = scroll.clone();
    View::new(crate::platform::fixture::facts(), move |ui| {
        if let Err(error) = list.smooth(ui.motion(), spec) {
            ui.fail(error);
        }
        ui.scroll(
            list.clone(),
            Modifier::new().size(100.dp(), 100.dp()),
            |ui| {
                for row in 0..10 {
                    ui.scoped(row, Modifier::new(), |ui| {
                        ui.spacer(100.dp(), 40.dp(), Modifier::new().background(Color::WHITE));
                    });
                }
            },
        );
    })
}

#[test]
fn a_smooth_scroll_toward_the_end_plays_in_the_slot_of_the_content_clamped_to_it() -> TestResult {
    let bouncy = crate::spring!(Duration::from_millis(300), 0.3);
    let tween = Tween::new(Duration::from_millis(300));
    for (spec, passes) in [(MotionSpec::Spring(bouncy), true), (tween.into(), false)] {
        let scroll = ScrollState::new(Axis::Vertical);
        let mut view = smooth_rows(&scroll, spec);
        let mut scene = Scene::default();
        show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
        scroll.jump_to(260.dp());
        show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
        view.input(InputEvent::Scroll {
            at: Point::new(10.dp(), 10.dp()),
            delta: Point::new(Dp::ZERO, 100.dp()),
        });
        let update = frame(&mut view, WINDOW_SIZE, 0)?;
        assert_eq!(motions(&update), 1, "{spec:?}");
        scene.show(update)?;
        assert!(!view.needs_frame(), "{spec:?}");
        let animation = scroll.animation().required("a scroll animation")?;
        let mut passed = false;
        for ms in (10..=2_500).step_by(10) {
            show_frame(&mut scene, &mut view, WINDOW_SIZE, ms)?;
            passed |= animation.get() > scroll.maximum();
            let [_root, content, ..] = scene.groups()[..] else {
                return Err(TestError::Absent("the root and the content as groups"));
            };
            assert_eq!(
                content.transform.translation,
                Point::new(Dp::ZERO, -scroll.offset()),
                "{spec:?} at {ms} ms"
            );
        }
        assert_eq!(passed, passes, "{spec:?}");
        assert_eq!(scroll.offset(), 300.dp(), "{spec:?}");
    }
    Ok(())
}

/// A view with a white card that scales in from 0.8 on a spring with a damping ratio of 0.5, which
/// overshoots the final scale before it comes to rest. When `filtered` is true, the card is inside
/// a node blurred by 2 dp, whose content is filtered as one image.
fn scaling_card(filtered: bool) -> View {
    View::new(crate::platform::fixture::facts(), move |ui| {
        let spring = crate::spring!(Duration::from_millis(800), 0.5);
        let entering = Presence::default().scale(crate::constant!(ScaleFactor::try_new(0.8)));
        let layer = Modifier::new();
        ui.layout(
            Axis::Vertical,
            if filtered { layer.blur(2.dp()) } else { layer },
            |ui| {
                ui.spacer(
                    100.dp(),
                    60.dp(),
                    Modifier::new()
                        .offset(30.dp(), 20.dp())
                        .background(Color::WHITE)
                        .transition(Transition::new(spring).enter(entering)),
                );
            },
        );
    })
}

#[test]
fn a_scaling_presence_plays_in_one_slot_with_the_transforms_that_painting_gives() -> TestResult {
    let first_transform = |scene: &Scene| {
        (scene.commands().into_iter()).find_map(|command| match command {
            DrawCommand::Transform(transform) => Some(transform),
            _ => None,
        })
    };
    let mut view = scaling_card(false);
    let mut scene = Scene::default();
    let update = frame(&mut view, WINDOW_SIZE, 0)?;
    assert_eq!(motions(&update), 1);
    scene.show(update)?;
    let painted = view.stats().painted_components;
    let mut painting = scaling_card(true);
    let mut reference = Scene::default();
    for ms in [0, 40, 90, 150, 220, 300, 400, 520, 680, 900] {
        assert!(
            frame(&mut view, WINDOW_SIZE, ms)?.changes.is_empty(),
            "{ms} ms"
        );
        show_frame(&mut reference, &mut painting, WINDOW_SIZE, ms)?;
        let shown = first_transform(scene.at(ms));
        assert!(shown.is_some(), "{ms} ms");
        assert_eq!(shown, first_transform(&reference), "{ms} ms");
    }
    assert_eq!(view.stats().painted_components, painted);
    Ok(())
}
