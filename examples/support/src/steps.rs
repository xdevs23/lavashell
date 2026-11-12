// SPDX-License-Identifier: GPL-3.0-only
use crate::Error;
use crate::frame_log::record;
use lavashell::core::surface::SurfaceId;
use lavashell::prelude::*;
use lavashell_test_platform::{HostScript, RecordedFrame, is_at_rest};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::{cell::RefCell, mem, rc::Rc, time::Duration};

/// Actions that an app plays on its view one after another, each once the view has come to rest after the one
/// before. Each action has a label for the frames that show it.
pub struct Steps<L, E> {
    waiting: VecDeque<Step<L, E>>,
}

type Action<E> = Box<dyn FnOnce(&mut View) -> Result<(), E>>;

struct Step<L, E> {
    label: L,
    action: Action<E>,
}

/// The steps of an app while it runs.
struct Playing<L, E> {
    waiting: VecDeque<Step<L, E>>,
    phase: Phase<E>,
    /// The presentation time of the frame that first shows each step that ran, with the label of the step.
    started: Vec<(Duration, L)>,
}

/// How far the next step got.
enum Phase<E> {
    /// The next step is queued once a frame shows the view at rest.
    Ready,
    /// The next step is queued on the view and has not run yet.
    Queued,
    /// A step failed, so no later step runs.
    Failed(E),
}

impl<L, E> Default for Steps<L, E> {
    fn default() -> Self {
        Self {
            waiting: VecDeque::new(),
        }
    }
}

impl<L: Clone + 'static, E: From<Error> + 'static> Steps<L, E> {
    /// Adds `action` as the last step, whose frames are labeled `label`.
    pub fn push(&mut self, label: L, action: impl FnOnce(&mut View) -> Result<(), E> + 'static) {
        self.waiting.push_back(Step {
            label,
            action: Box::new(action),
        });
    }

    /// Records `app` on a test platform that plays `script`, while the steps play on its views: each step runs on
    /// the view of a surface once a frame shows that view at rest. Returns each recorded frame that shows a step,
    /// with the label of that step. Replaces the view observer of `app`.
    ///
    /// # Errors
    /// Returns the failure of the first step that failed, an error when the recording fails, and
    /// [`Error::UnplayedSteps`] when the recording came to rest before every step ran.
    pub fn record(self, app: App, script: HostScript) -> Result<Vec<(L, RecordedFrame)>, E> {
        let playing = Rc::new(RefCell::new(Playing {
            waiting: self.waiting,
            phase: Phase::Ready,
            started: Vec::new(),
        }));
        let actions = app.view_actions();
        let observed = Rc::clone(&playing);
        let app = app.view_observer(move |surface, _, _, view| {
            observe(&observed, &actions, surface, view)
        });
        let frames = record(app, script)?.into_frames();
        let mut played = playing.borrow_mut();
        let unplayed = match mem::replace(&mut played.phase, Phase::Ready) {
            Phase::Ready => played.waiting.len(),
            Phase::Queued => played.waiting.len() + 1,
            Phase::Failed(failure) => return Err(failure),
        };
        if unplayed > 0 {
            return Err(Error::UnplayedSteps(unplayed).into());
        }
        Ok(label(frames, &played.started))
    }
}

/// Labels each frame with the newest step that started at or before its presentation. A step during which no frame
/// was presented is shown by the newest frame of each surface presented before it started.
fn label<L: Clone>(
    frames: Vec<RecordedFrame>,
    started: &[(Duration, L)],
) -> Vec<(L, RecordedFrame)> {
    let step = |frame: &RecordedFrame| {
        (started.iter()).rposition(|(start, _)| *start <= frame.time.presentation)
    };
    let shown: BTreeSet<usize> = frames.iter().filter_map(step).collect();
    let mut labeled: Vec<_> = (0..started.len())
        .filter(|index| !shown.contains(index))
        .flat_map(|index| {
            on_screen(&frames, started[index].0).map(move |frame| (index, frame.clone()))
        })
        .collect();
    labeled.extend((frames.into_iter()).filter_map(|frame| Some((step(&frame)?, frame))));
    labeled.sort_by_key(|&(index, _)| index);
    (labeled.into_iter())
        .map(|(index, frame)| (started[index].1.clone(), frame))
        .collect()
}

/// The newest frame of each surface presented before `time`, in the order of the surfaces.
fn on_screen(frames: &[RecordedFrame], time: Duration) -> impl Iterator<Item = &RecordedFrame> {
    (frames.iter())
        .filter(|frame| frame.time.presentation < time)
        .map(|frame| (frame.surface, frame))
        .collect::<BTreeMap<_, _>>()
        .into_values()
}

#[cfg(test)]
mod tests;

/// Queues the next step on `view` once a frame shows `view` at rest. The step records the presentation time of the
/// frame that it changes.
fn observe<L: 'static, E: 'static>(
    playing: &Rc<RefCell<Playing<L, E>>>,
    actions: &ViewActions,
    surface: SurfaceId,
    view: &View,
) {
    let mut state = playing.borrow_mut();
    let at_rest = is_at_rest(view.needs_visual_frame(), view.next_deadline());
    if !matches!(state.phase, Phase::Ready) || !at_rest {
        return;
    }
    let Some(step) = state.waiting.pop_front() else {
        return;
    };
    state.phase = Phase::Queued;
    let playing = Rc::clone(playing);
    actions.dispatch(surface, move |view| {
        let Step { label, action } = step;
        let started = view.motion().now();
        let ran = action(view);
        let mut state = playing.borrow_mut();
        state.started.push((started, label));
        state.phase = match ran {
            Ok(()) => Phase::Ready,
            Err(failure) => Phase::Failed(failure),
        };
    });
}
