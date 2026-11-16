// SPDX-License-Identifier: GPL-3.0-only
use crate::Error;
use crate::frame_log::record;
use lavashell::core::surface::SurfaceId;
use lavashell::prelude::*;
use lavashell_test_platform::{HostScript, RecordedFrame, is_at_rest};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::{cell::RefCell, mem, rc::Rc, time::Duration};

pub struct Steps<L, E> {
    waiting: VecDeque<Step<L, E>>,
}

type Action<E> = Box<dyn FnOnce(&mut View) -> Result<(), E>>;

struct Step<L, E> {
    label: L,
    action: Action<E>,
}

struct Playing<L, E> {
    waiting: VecDeque<Step<L, E>>,
    phase: Phase<E>,
    started: Vec<(Duration, L)>,
}

enum Phase<E> {
    Ready,
    Queued,
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
    pub fn push(&mut self, label: L, action: impl FnOnce(&mut View) -> Result<(), E> + 'static) {
        self.waiting.push_back(Step {
            label,
            action: Box::new(action),
        });
    }

    pub fn record(self, app: App, script: HostScript) -> Result<Vec<(L, RecordedFrame)>, E> {
        let playing = Rc::new(RefCell::new(Playing {
            waiting: self.waiting,
            phase: Phase::Ready,
            started: Vec::new(),
        }));
        let actions = app.view_actions();
        let observed = Rc::clone(&playing);
        let app =
            app.view_observer(move |surface, _, view| observe(&observed, &actions, surface, view));
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

fn on_screen(frames: &[RecordedFrame], time: Duration) -> impl Iterator<Item = &RecordedFrame> {
    (frames.iter())
        .filter(|frame| frame.time.presentation < time)
        .map(|frame| (frame.surface, frame))
        .collect::<BTreeMap<_, _>>()
        .into_values()
}

#[cfg(test)]
mod tests;

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
