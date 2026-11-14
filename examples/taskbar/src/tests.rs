// SPDX-License-Identifier: GPL-3.0-only
use crate::{
    CopyKey, Error, copy,
    islands::{Island, TRACK},
    look::{SCREEN, SPACING},
    model::{App, Daylight, Taskbar},
    pause, player,
    script::{CUES, Cue, Script},
};
use lavashell::prelude::*;
use lavashell_example_support::testing::{capture_path, control, interaction, write_first_frames};
use lavashell_example_support::{FrameInterval, Start, Steps, default_app};
use std::cell::{Cell, RefCell};
use std::{collections::BTreeSet, path::PathBuf, rc::Rc, time::Duration};

const ISLANDS: [Island; 3] = [Island::Apps, Island::Media, Island::Status];

/// The time between the frames that a check of the islands reads inside a motion.
const SAMPLES: FrameInterval = constant!(FrameInterval::per_second(60));

#[derive(Debug, thiserror::Error)]
enum TestError {
    #[error(transparent)]
    Taskbar(#[from] Error),
    #[error(transparent)]
    Example(#[from] lavashell_example_support::Error),
    #[error("cannot find the test binary, beside which the captures are written")]
    TestBinary(#[source] std::io::Error),
}

#[derive(Default)]
struct Seen {
    rest: Cell<[Rect; 3]>,
    entering: Cell<Rect>,
    controls: Cell<Rect>,
    compositions: Cell<u64>,
}

struct Play {
    steps: Steps<Taskbar, PathBuf, TestError>,
    script: Script,
    captures: PathBuf,
}

/// The app that shows `taskbar` on the screen of the example.
fn taskbar_app(taskbar: &Taskbar) -> Result<lavashell::prelude::App, TestError> {
    Ok(player::app(default_app()?, taskbar, SCREEN, false)?)
}

impl Play {
    /// Plays the next cue of the script once `start` holds.
    fn cue(&mut self, start: Start) {
        let cue = self.script.next_cue();
        (self.steps).push(start, move |view, _, taskbar| Ok(cue.apply(taskbar, view)?));
    }

    /// Writes the frame of the view, taken once `start` holds, to the capture `name`.
    fn capture(&mut self, start: Start, name: &str) {
        self.steps.mark(start, capture_path(&self.captures, name));
    }
}

/// In the frame halfway to the next deadline that the view waits for in the frame that shows the step before, while
/// the motions that end at that deadline still run.
fn midway() -> Start {
    Start::after_reading(|view| {
        let now = view.motion().now();
        (view.next_deadline()).map(|deadline| deadline.saturating_sub(now) / 2)
    })
}

fn island(view: &View, island: Island) -> Result<Rect, Error> {
    Ok(control(view, island.tag())?)
}

fn gap(view: &View, left: Island, right: Island) -> Result<Dp, Error> {
    let (left, right) = (island(view, left)?, island(view, right)?);
    Ok(right.x - (left.x + left.width))
}

/// Once a frame shows a gap between the apps and the media island for which `reached` returns true.
fn gap_reached(reached: fn(Dp) -> bool) -> Start {
    Start::sampled(SAMPLES, move |view| {
        gap(view, Island::Apps, Island::Media).is_ok_and(reached)
    })
}

fn pixel() -> Dp {
    Viewport::new(SCREEN, Density::ONE).pixel()
}

#[test]
fn the_islands_trip_dock_and_morph_and_return_to_rest() -> Result<(), TestError> {
    let captures = std::env::current_exe()
        .map_err(TestError::TestBinary)?
        .with_file_name("taskbar-captures");
    let taskbar = Taskbar::new()?;
    let mut play = Play {
        steps: Steps::new(taskbar.clone()),
        script: Script::default(),
        captures,
    };
    let seen = Rc::new(Seen::default());
    trip_and_dock(&mut play, &seen);
    morph(&mut play, &seen);
    controls_and_focus(&mut play, &seen);
    daylight(&mut play, &seen);
    let snapped = pixel() / 2.0;
    let rest = Rc::clone(&seen);
    play.steps.check(Start::at_rest(), move |view, _, _| {
        for (which, bounds) in ISLANDS.into_iter().zip(rest.rest.get()) {
            let offset = island(view, which)?
                .vector()
                .sub(&bounds.vector())
                .max_abs();
            assert!(offset < snapped.get(), "{which:?} returns to its place");
        }
        Ok(())
    });
    play.steps.push(Start::at_rest(), |view, _, taskbar| {
        view.provide(&REDUCED_MOTION, true);
        Ok(Cue::Dock(true).apply(taskbar, view)?)
    });
    play.steps.check(Start::NEXT, move |view, _, _| {
        assert!(gap(view, Island::Apps, Island::Media)?.abs() < snapped);
        assert!(
            !view.motion().is_active(),
            "reduced motion settles every change"
        );
        Ok(())
    });
    let frames = play.steps.play(taskbar_app(&taskbar)?, Density::ONE)?;
    Ok(write_first_frames(frames)?)
}

/// Plays the start of the trip, the docking of the apps and the media island and their parting.
fn trip_and_dock(play: &mut Play, seen: &Rc<Seen>) {
    let snapped = pixel() / 2.0;
    let rest = Rc::clone(seen);
    play.steps.check(Start::at_rest(), move |view, _, _| {
        let [apps, media, status] = ISLANDS.map(|which| island(view, which));
        rest.rest.set([apps?, media?, status?]);
        assert!(
            gap(view, Island::Apps, Island::Media)? > SPACING
                && gap(view, Island::Media, Island::Status)? > SPACING
        );
        Ok(())
    });
    play.capture(Start::at_rest(), "rest");

    play.cue(Start::at_rest());
    let entering = Rc::clone(seen);
    play.steps.check(midway(), move |view, _, _| {
        let status = island(view, Island::Status)?;
        assert!(
            status.width > entering.rest.get()[2].width,
            "the status island grows with its new part"
        );
        entering.entering.set(status);
        Ok(())
    });
    play.capture(Start::NEXT, "enter");
    let entered = Rc::clone(seen);
    play.steps.check(Start::at_rest(), move |view, _, _| {
        assert!(island(view, Island::Status)?.width > entered.entering.get().width);
        Ok(())
    });

    play.cue(Start::at_rest());
    play.steps
        .check(gap_reached(|gap| gap < SPACING), |view, _, _| {
            assert!(
                gap(view, Island::Apps, Island::Media)? > Dp::ZERO,
                "the islands fuse before they touch"
            );
            Ok(())
        });
    play.capture(Start::NEXT, "merge");
    play.steps.check(Start::at_rest(), move |view, _, _| {
        assert!(
            gap(view, Island::Apps, Island::Media)?.abs() < snapped,
            "the docked islands touch"
        );
        Ok(())
    });
    play.capture(Start::at_rest(), "merged");
    play.cue(Start::at_rest());
    play.steps
        .check(gap_reached(|gap| gap > Dp::ZERO), |view, _, _| {
            let gap = gap(view, Island::Apps, Island::Media)?;
            assert!(
                gap < SPACING,
                "the islands part after they move apart: {gap:?}"
            );
            Ok(())
        });
    play.capture(Start::NEXT, "split");
    play.steps.check(Start::at_rest(), |view, _, _| {
        assert!(gap(view, Island::Apps, Island::Media)? > SPACING);
        Ok(())
    });
}

/// Plays the opening of the media island around the card and its closing.
fn morph(play: &mut Play, seen: &Rc<Seen>) {
    let snapped = pixel() / 2.0;
    let controls = Rc::clone(seen);
    play.steps.check(Start::at_rest(), move |view, _, _| {
        controls.controls.set(control(view, TRACK)?);
        Ok(())
    });
    play.cue(Start::at_rest());
    let opening = Rc::clone(seen);
    play.steps.check(midway(), move |view, _, _| {
        assert!(
            island(view, Island::Media)?.height > opening.rest.get()[1].height,
            "the media island opens"
        );
        let moved = control(view, TRACK)?.y - opening.controls.get().y;
        assert!(moved.abs() < pixel(), "the controls do not move: {moved:?}");
        Ok(())
    });
    play.capture(Start::NEXT, "morph");
    let open = Rc::clone(seen);
    play.steps.check(Start::at_rest(), move |view, _, _| {
        assert!(
            island(view, Island::Media)?.height > open.rest.get()[1].height,
            "the media island rests open around the card"
        );
        Ok(())
    });
    play.capture(Start::at_rest(), "card");
    play.cue(Start::at_rest());
    let closed = Rc::clone(seen);
    play.steps.check(Start::at_rest(), move |view, _, _| {
        let media = closed.rest.get()[1];
        assert!((island(view, Island::Media)?.height - media.height).abs() < snapped);
        Ok(())
    });
}

/// Adds a step that stores the number of compositions of the view root in `seen`.
fn count_compositions(play: &mut Play, seen: &Rc<Seen>) {
    let counted = Rc::clone(seen);
    play.steps.check(Start::at_rest(), move |view, _, _| {
        counted.compositions.set(view.stats().root_compositions);
        Ok(())
    });
}

/// Plays the press and release of an app slot, the three tabs and the tap on the map.
fn controls_and_focus(play: &mut Play, seen: &Rc<Seen>) {
    count_compositions(play, seen);
    play.cue(Start::at_rest());
    play.steps.check(Start::at_rest(), |view, _, _| {
        let pressed = interaction(view, App::Music.name())?;
        assert!(pressed.pointer.active && pressed.focus.focused);
        assert!(!pressed.focus.visible, "a tap hides the focus ring");
        Ok(())
    });
    play.capture(Start::at_rest(), "press");
    play.cue(Start::at_rest());
    let tinted = Rc::clone(seen);
    play.steps.check(Start::at_rest(), move |view, _, taskbar| {
        assert_eq!(taskbar.foreground.get(), App::Music);
        assert_eq!(
            view.stats().root_compositions,
            tinted.compositions.get() + 1,
            "only the change of the selected app composes the taskbar again"
        );
        Ok(())
    });
    play.capture(Start::at_rest(), "tint");

    count_compositions(play, seen);
    play.cue(Start::at_rest());
    play.steps.check(Start::at_rest(), |view, _, _| {
        assert!(
            interaction(view, App::Radio.name())?.focus.visible,
            "a tab shows the focus ring on the next slot"
        );
        Ok(())
    });
    play.capture(Start::at_rest(), "focus");
    play.cue(Start::at_rest());
    play.cue(Start::at_rest());
    play.steps.check(Start::at_rest(), |view, _, _| {
        assert!(interaction(view, copy(CopyKey::Previous))?.focus.visible);
        Ok(())
    });
    play.cue(Start::at_rest());
    let painted = Rc::clone(seen);
    play.steps.check(Start::at_rest(), move |view, _, _| {
        assert!(view.focus().is_none(), "a tap on the map clears the focus");
        assert_eq!(
            view.stats().root_compositions,
            painted.compositions.get(),
            "focus changes only paint"
        );
        Ok(())
    });
}

/// Plays the end of the trip, the change of the foreground app and the change to day and back.
fn daylight(play: &mut Play, seen: &Rc<Seen>) {
    play.cue(Start::at_rest());
    play.cue(Start::at_rest());
    count_compositions(play, seen);
    play.cue(Start::at_rest());
    play.capture(midway(), "to-day");
    let tinted = Rc::clone(seen);
    play.steps.check(Start::at_rest(), move |view, _, taskbar| {
        assert_eq!(taskbar.daylight.get(), Daylight::Day);
        assert_eq!(
            view.stats().root_compositions,
            tinted.compositions.get() + 1,
            "the glass of the bar moves to the day tint without composing again"
        );
        Ok(())
    });
    play.capture(Start::at_rest(), "day");
    play.cue(Start::at_rest());
    play.capture(midway(), "to-night");
}

/// The time from the start of each cue of the script until the scene comes to rest after it.
fn motion_times() -> Result<Vec<Duration>, TestError> {
    let taskbar = Taskbar::new()?;
    let times = Rc::new(RefCell::new(Vec::new()));
    let mut steps = Steps::<_, (), TestError>::new(taskbar.clone());
    for cue in CUES {
        let started = Rc::new(Cell::new(Duration::ZERO));
        let start = Rc::clone(&started);
        steps.push(Start::at_rest(), move |view, _, taskbar| {
            start.set(view.motion().now());
            Ok(cue.apply(taskbar, view)?)
        });
        let times = Rc::clone(&times);
        steps.check(Start::at_rest(), move |view, _, _| {
            times
                .borrow_mut()
                .push(view.motion().now().saturating_sub(started.get()));
            Ok(())
        });
    }
    let _frames_of_the_cues = steps.play(taskbar_app(&taskbar)?, Density::ONE)?;
    Ok(times.take())
}

#[test]
fn reduced_motion_keeps_each_cue_on_screen_as_long_as_its_motion_takes() -> Result<(), TestError> {
    let motion_times = motion_times()?;
    let taskbar = Taskbar::new()?;
    let mut steps = Steps::<_, (), TestError>::new(taskbar.clone());
    steps.push(Start::at_rest(), |view, _, _| {
        view.provide(&REDUCED_MOTION, true);
        Ok(())
    });
    let mut start = Start::NEXT;
    for (cue, moving) in CUES.into_iter().zip(motion_times) {
        steps.push(start, move |view, _, taskbar| Ok(cue.apply(taskbar, view)?));
        steps.check(Start::NEXT, move |view, _, _| {
            assert_eq!(pause(view.motion()), moving, "{cue:?}");
            Ok(())
        });
        start = Start::after(moving);
    }
    let _frames_of_the_cues = steps.play(taskbar_app(&taskbar)?, Density::ONE)?;
    Ok(())
}

#[test]
fn a_recording_shows_every_cue_in_at_least_one_frame() -> Result<(), Error> {
    let interval = constant!(FrameInterval::per_second(10));
    let shown: BTreeSet<_> = (player::record_cues(interval, SCREEN)?.into_iter())
        .map(|(cue, _)| cue)
        .collect();
    assert_eq!(shown, (0..CUES.len()).collect());
    Ok(())
}
