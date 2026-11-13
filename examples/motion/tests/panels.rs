// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]
use lavashell::prelude::*;
use lavashell_example_support::testing::{capture_path, click, control, drag, write_first_frames};
use lavashell_example_support::{Start, Steps, default_app, node};
use lavashell_motion::{CopyKey, Error, app, copy, model::Demo, ui::WINDOW};
use std::{cell::Cell, path::Path, rc::Rc, time::Duration};

/// The frames written as captures: each frame of the intro, and the first frame after the interactions.
#[derive(Clone, Copy, PartialEq)]
enum Capture {
    Intro,
    Uncaptured,
    Interactions,
}

type Play = Steps<Demo, Capture, Error>;

#[test]
fn panels_retarget_pause_seek_and_settle_with_reduced_motion() -> Result<(), Error> {
    let demo = Demo::new(true)?;
    let mut play = Play::new(demo.clone());
    play.mark(Start::NEXT, Capture::Intro);
    retarget(&mut play, &demo);
    pause_seek_and_play(&mut play);
    timeline_semantics(&mut play);
    morph_and_spring(&mut play);
    reduced_motion(&mut play);

    let failure = Rc::new(Cell::new(None));
    let failed = Rc::clone(&failure);
    let app = app(default_app()?, &demo, WINDOW, false, move |error| {
        failed.set(Some(Error::Motion(error)));
    })?;
    let played = play.play(app, Density::ONE);
    if let Some(failure) = failure.take() {
        return Err(failure);
    }
    let frames = played?;
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

/// Plays the end of the autoplay and the collapse of the panels. A click on a panel retargets that collapse. The
/// autoplay stops in the frame that shows the intro finished, before it advances by another step.
fn retarget(play: &mut Play, demo: &Demo) {
    let finished = demo.clone();
    let intro_finished =
        Start::when(move |_| finished.spring.is_finished() && finished.morph_finished());
    play.check(intro_finished, |view, _, demo| {
        assert!(view.bounds_of("Undertow").is_some());
        demo.autoplay.set(false);
        Ok(())
    });
    play.push(Start::NEXT, |view, _, demo| {
        let autoplay = node(view, &Role::Switch, copy(CopyKey::AutoPlay))?;
        assert_eq!(autoplay.state.semantics.checked, Some(Checked::False));
        assert!(!demo.autoplay.get());
        Ok(click(view, copy(CopyKey::Collapse))?)
    });
    play.mark(Start::NEXT, Capture::Uncaptured);
    let before = Rc::new(Cell::new(Rect::default()));
    let collapsing = Rc::clone(&before);
    play.push(
        Start::after(Duration::from_millis(100)),
        move |view, _, _| {
            let bounds = control(view, "Undertow")?;
            assert!(bounds.width > 212.dp() && bounds.width < 478.dp());
            collapsing.set(bounds);
            Ok(click(view, "Undertow")?)
        },
    );
    play.check(Start::NEXT, move |view, _, demo| {
        let offset = control(view, "Undertow")?
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
        click(view, copy(CopyKey::Pause))?;
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
        let at = control(view, "timeline-seek")?.center();
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
            Ok(click(view, copy(CopyKey::Play))?)
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
            "keyboard seek must start from current animated progress"
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
        click(view, copy(CopyKey::Morph))?;
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
            click(view, copy(CopyKey::Morph))?;
            assert_eq!(demo.shape(), moving);

            let slider = control(view, "spring-target")?;
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
        Ok(click(view, copy(CopyKey::ReducedMotion))?)
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
