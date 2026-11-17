// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]
use lavashell::prelude::*;
use lavashell_example_support::testing::{
    RecordedFrame, Shaping, capture_path, click, control, drag, write_first_frames,
};
use lavashell_example_support::{Start, Steps, ThemeChoice, default_app, node};
use lavashell_motion::{CopyKey, Error, Tag, app, copy, model::Demo, ui::WINDOW};
use std::{cell::Cell, path::Path, rc::Rc, time::Duration};

#[derive(Clone, Copy, PartialEq)]
enum Capture {
    Intro,
    Uncaptured,
    Interactions,
}

type Play = Steps<Demo, Capture, Error>;

fn play(play: Play, demo: &Demo) -> Result<Vec<(Capture, RecordedFrame)>, Error> {
    let failure = Rc::new(Cell::new(None));
    let failed = Rc::clone(&failure);
    let app = app(default_app()?, demo, WINDOW, false, move |error| {
        failed.set(Some(Error::Motion(error)));
    })?;
    let played = play.play(app, Density::ONE);
    failure.take().map_or(played, Err)
}

#[test]
fn panels_retarget_pause_seek_and_settle_with_reduced_motion() -> Result<(), Error> {
    let demo = Demo::new(true, ThemeChoice::System)?;
    let mut steps = Play::new(demo.clone());
    steps.mark_sequence(Start::NEXT, Capture::Intro);
    retarget(&mut steps, &demo);
    pause_seek_and_play(&mut steps);
    timeline_semantics(&mut steps);
    morph_and_spring(&mut steps);
    reduced_motion(&mut steps);

    let frames = play(steps, &demo)?;
    let captures = Path::new(env!("CARGO_TARGET_TMPDIR")).join("motion");
    let (intro, shown): (Vec<_>, Vec<_>) =
        (frames.into_iter()).partition(|(capture, _)| *capture == Capture::Intro);
    let intro = (intro.into_iter().enumerate())
        .map(|(index, (_, frame))| (capture_path(&captures, &index.to_string()), frame));
    let interactions = (shown.into_iter())
        .filter(|(capture, _)| *capture == Capture::Interactions)
        .map(|(_, frame)| (capture_path(&captures, "interactions"), frame));
    Ok(write_first_frames(intro.chain(interactions).collect())?)
}

fn retarget(play: &mut Play, demo: &Demo) {
    let finished = demo.clone();
    let intro_finished =
        Start::when(move |_| finished.spring.is_finished() && finished.morph_finished());
    let expanded = Rc::new(Cell::new(Rect::default()));
    let measured = Rc::clone(&expanded);
    play.check(intro_finished, move |view, _, demo| {
        measured.set(control(view, Tag::Card)?);
        demo.autoplay.set(false);
        Ok(())
    });
    play.push(Start::NEXT, |view, _, demo| {
        let autoplay = node(view, &Role::Switch, copy(CopyKey::AutoPlay))?;
        assert_eq!(autoplay.state.semantics.checked, Some(Checked::False));
        assert!(!demo.autoplay.get());
        Ok(click(view, Tag::Control(CopyKey::Collapse))?)
    });
    play.mark(Start::NEXT, Capture::Uncaptured);
    let before = Rc::new(Cell::new(Rect::default()));
    let collapsing = Rc::clone(&before);
    play.push(
        Start::after(Duration::from_millis(100)),
        move |view, _, _| {
            let bounds = control(view, Tag::Card)?;
            assert!(
                bounds.width < expanded.get().width && view.motion().is_active(),
                "the card is still collapsing"
            );
            collapsing.set(bounds);
            Ok(click(view, Tag::Card)?)
        },
    );
    play.check(Start::NEXT, move |view, _, demo| {
        let offset = control(view, Tag::Card)?
            .vector()
            .sub(&before.get().vector());
        assert!(offset.max_abs() < 0.001);
        assert!(demo.expanded.get());
        Ok(())
    });
}

fn pause_seek_and_play(play: &mut Play) {
    let held = Rc::new(Cell::new(0.0));
    let pausing = Rc::clone(&held);
    play.push(Start::NEXT, move |view, _, demo| {
        click(view, Tag::Control(CopyKey::Pause))?;
        pausing.set(demo.timeline.get());
        Ok(())
    });
    play.check(
        Start::after(Duration::from_millis(300)),
        move |_, _, demo| {
            assert_eq!(demo.timeline.get().to_bits(), held.get().to_bits());
            Ok(())
        },
    );
    let sought = Rc::new(Cell::new(0.0));
    let seeking = Rc::clone(&sought);
    play.push(Start::NEXT, move |view, _, demo| {
        let at = control(view, Tag::TimelineSeek)?.center();
        drag(view, at, at);
        let timeline = demo.timeline.get();
        assert!(timeline > 0.45 && timeline < 0.6);
        seeking.set(timeline);
        Ok(())
    });
    let paused = Rc::clone(&sought);
    play.push(
        Start::after(Duration::from_millis(200)),
        move |view, _, demo| {
            assert_eq!(demo.timeline.get().to_bits(), paused.get().to_bits());
            Ok(click(view, Tag::Control(CopyKey::Play))?)
        },
    );
    play.check(
        Start::after(Duration::from_millis(100)),
        move |_, _, demo| {
            assert!(demo.timeline.get() > sought.get());
            Ok(())
        },
    );

    let compositions = |view: &View| {
        (
            view.stats().root_compositions,
            view.stats().component_compositions,
        )
    };
    let composed = Rc::new(Cell::new((0, 0)));
    let counted = Rc::clone(&composed);
    play.check(Start::NEXT, move |view, _, _| {
        counted.set(compositions(view));
        Ok(())
    });
    play.check(
        Start::after(Duration::from_millis(20)),
        move |view, _, _| {
            assert_eq!(
                compositions(view),
                composed.get(),
                "animated semantics must preserve paint-only motion"
            );
            Ok(())
        },
    );
}

fn timeline_semantics(play: &mut Play) {
    play.push(Start::NEXT, |view, _, demo| {
        let timeline = node(view, &Role::Slider, copy(CopyKey::TimelineTitle))?.clone();
        let length = demo.timeline_length();
        let playback_progress = demo.timeline.elapsed().as_secs_f64() / length.as_secs_f64();
        assert!((0.0..=1.0).contains(&playback_progress));
        assert!(
            matches!(timeline.value, Some(AccessibleValue::Number(value))
                if (value - playback_progress).abs() <= f64::EPSILON),
            "semantic progress must match the playback position shown by the slider"
        );
        let elapsed = demo.timeline.elapsed();
        assert_eq!(
            view.accessibility_action(ActionRequest {
                target: timeline.id,
                action: AccessibilityAction::Focus
            }),
            Ok(ActionOutcome::Performed)
        );
        view.input(InputEvent::Key {
            key: Key::Right,
            shift: false,
        });
        let stepped = demo.timeline.elapsed();
        assert!(
            stepped > elapsed,
            "keyboard seek must start from the animated progress the view shows"
        );
        assert_eq!(
            view.accessibility_action(ActionRequest {
                target: timeline.id,
                action: AccessibilityAction::Increment,
            }),
            Ok(ActionOutcome::Performed)
        );
        assert!(demo.timeline.elapsed() > stepped);
        Ok(())
    });
}

fn morph_and_spring(play: &mut Play) {
    let shape = Rc::new(Cell::new(None));
    let clicked = Rc::clone(&shape);
    play.push(Start::NEXT, move |view, _, demo| {
        let before = demo.shape();
        assert!(before.is_some());
        click(view, Tag::Control(CopyKey::Morph))?;
        assert_eq!(demo.shape(), before);
        clicked.set(before);
        Ok(())
    });
    let position = Rc::new(Cell::new(0.0));
    let dragged = Rc::clone(&position);
    play.push(
        Start::after(Duration::from_millis(180)),
        move |view, _, demo| {
            assert_ne!(demo.shape(), shape.take());
            let moving = demo.shape();
            click(view, Tag::Control(CopyKey::Morph))?;
            assert_eq!(demo.shape(), moving);

            let slider = control(view, Tag::SpringTarget)?;
            let at = Point::new(slider.x + slider.width * 0.75, slider.center().y);
            let spring = demo.spring.get();
            drag(view, at, at);
            assert!((demo.target.get() - 0.75).abs() < 0.01);
            assert_eq!(demo.spring.get().to_bits(), spring.to_bits());
            dragged.set(spring);
            Ok(())
        },
    );
    play.check(
        Start::after(Duration::from_millis(200)),
        move |_, _, demo| {
            assert_ne!(demo.spring.get().to_bits(), position.get().to_bits());
            Ok(())
        },
    );
}

fn reduced_motion(play: &mut Play) {
    play.push(Start::NEXT, |view, _, _| {
        Ok(click(view, Tag::Control(CopyKey::ReducedMotion))?)
    });
    play.check(Start::NEXT, |view, _, demo| {
        assert!(!view.motion().is_active());
        assert_eq!(demo.spring.get().to_bits(), demo.target.get().to_bits());
        Ok(())
    });
    play.mark(Start::NEXT, Capture::Interactions);
    play.push(Start::NEXT, |view, _, demo| {
        demo.set_reduced(false).map_err(Error::Motion)?;
        demo.restart().map_err(Error::Motion)?;
        view.provide(&REDUCED_MOTION, true);
        Ok(())
    });
    play.check(Start::after(Duration::from_millis(100)), |view, _, _| {
        assert!(
            !view.motion().is_active(),
            "the example must honor the effective reduced-motion preference"
        );
        let reduced = node(view, &Role::Switch, copy(CopyKey::ReducedMotion))?;
        assert_eq!(reduced.state.semantics.checked, Some(Checked::True));
        assert!(
            reduced.state.semantics.disabled,
            "a local demo toggle cannot undo an enforced accessibility preference"
        );
        Ok(())
    });
}

fn page_fill(
    shaping: &Shaping,
    view: &mut View,
    viewport: Viewport,
) -> Result<Option<DrawCommand>, Error> {
    let tree = shaping.scene(view, viewport)?;
    let now = view.motion().now();
    Ok(tree.at(now).leaf_commands().next().cloned())
}

#[test]
fn the_theme_switch_repaints_the_window() -> Result<(), Error> {
    let demo = Demo::new(false, ThemeChoice::System)?;
    let mut steps = Play::new(demo.clone());
    let shaping = Shaping::new()?;
    steps.push(Start::NEXT, move |view, viewport, _| {
        let by_system = page_fill(&shaping, view, viewport)?;
        click(view, ThemeChoice::Light)?;
        assert_ne!(page_fill(&shaping, view, viewport)?, by_system);
        click(view, ThemeChoice::Dark)?;
        assert_eq!(page_fill(&shaping, view, viewport)?, by_system);
        Ok(())
    });
    play(steps, &demo).map(drop)
}

fn back_gesture(view: &View) -> Result<impl Fn(f64) -> Point + use<>, Error> {
    let stage = control(view, Tag::SettingsStage)?;
    let start = Point::new(stage.x + stage.width * 0.1, stage.y + stage.height * 0.95);
    Ok(move |share: f64| start.offset(stage.width * share, Dp::ZERO))
}

#[test]
fn the_back_gesture_sets_the_crossing_progress_also_backwards_and_snaps_with_reduced_motion()
-> Result<(), Error> {
    let demo = Demo::new(false, ThemeChoice::System)?;
    let mut steps = Play::new(demo.clone());
    steps.push(Start::NEXT, |view, _, demo| {
        let at = back_gesture(view)?;
        let progress = &demo.back.progress;
        view.input(InputEvent::Press(at(0.0)));
        view.input(InputEvent::Move(at(0.6)));
        assert!((progress.get() - 0.6).abs() < 0.01);
        view.input(InputEvent::Move(at(0.3)));
        assert!((progress.get() - 0.3).abs() < 0.01);
        view.input(InputEvent::Release(at(0.3)));
        Ok(())
    });
    steps.check(Start::after(Duration::from_millis(100)), |_, _, demo| {
        let progress = &demo.back.progress;
        assert!(progress.get() < 0.3 && progress.target() < 0.3);
        Ok(())
    });
    steps.push(Start::NEXT, |_, _, demo| {
        demo.set_reduced(true).map_err(Error::Motion)
    });
    steps.push(Start::NEXT, |view, _, demo| {
        let at = back_gesture(view)?;
        let progress = &demo.back.progress;
        view.input(InputEvent::Press(at(0.0)));
        view.input(InputEvent::Move(at(0.7)));
        assert_eq!(progress.get().to_bits(), 0.0_f64.to_bits());
        view.input(InputEvent::Release(at(0.7)));
        assert_eq!(progress.get().to_bits(), 1.0_f64.to_bits());
        Ok(())
    });
    steps.check(Start::NEXT, |_, _, demo| {
        assert!(!demo.back.shows_display());
        Ok(())
    });
    play(steps, &demo).map(drop)
}

#[test]
fn a_drag_over_the_covered_display_row_moves_the_display_page() -> Result<(), Error> {
    let demo = Demo::new(false, ThemeChoice::System)?;
    let mut steps = Play::new(demo.clone());
    steps.push(Start::NEXT, |view, _, demo| {
        let stage = control(view, Tag::SettingsStage)?;
        let start = control(view, Tag::DisplayRow)?.center();
        view.input(InputEvent::Press(start));
        view.input(InputEvent::Move(start.offset(stage.width * 0.5, Dp::ZERO)));
        assert!(demo.back.progress.get() > 0.0);
        Ok(())
    });
    play(steps, &demo).map(drop)
}

#[test]
fn the_back_control_and_the_display_row_animate_the_crossing_both_ways() -> Result<(), Error> {
    let demo = Demo::new(false, ThemeChoice::System)?;
    let mut steps = Play::new(demo.clone());
    let crossing = |_: &View, _, demo: &Demo| {
        let progress = demo.back.progress.get();
        assert!(progress > 0.0 && progress < 1.0);
        Ok(())
    };
    steps.push(Start::NEXT, |view, _, _| {
        Ok(click(view, Tag::Control(CopyKey::Back))?)
    });
    steps.check(Start::after(Duration::from_millis(100)), crossing);
    steps.push(Start::after(Duration::from_secs(2)), |view, _, demo| {
        assert!(!demo.back.shows_display());
        Ok(click(view, Tag::DisplayRow)?)
    });
    steps.check(Start::after(Duration::from_millis(100)), crossing);
    steps.check(Start::after(Duration::from_secs(2)), |_, _, demo| {
        assert_eq!(demo.back.progress.get().to_bits(), 0.0_f64.to_bits());
        Ok(())
    });
    play(steps, &demo).map(drop)
}
