// SPDX-License-Identifier: GPL-3.0-only
use crate::Error;
use crate::frame_log::record;
use lavashell::core::frame::FrameTime;
use lavashell::core::surface::SurfaceId;
use lavashell::prelude::*;
use lavashell_test_platform::{
    FrameInterval, HostInput, HostScript, Pace, PacedLinks, RecordedFrame, is_at_rest,
};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::{cell::RefCell, cmp::Ordering, mem, rc::Rc, time::Duration};

pub struct Steps<C, L, E> {
    context: Rc<C>,
    waiting: VecDeque<Step<C, L, E>>,
    pub(crate) links: PacedLinks,
}

pub struct Start {
    delay: Option<Duration>,
    every: Option<FrameInterval>,
    condition: Option<Condition>,
}

type Condition = Box<dyn Fn(&View) -> bool>;

type Action<C, E> = Box<dyn FnOnce(&mut View, Viewport, &C) -> Result<(), E>>;
type Check<C, E> = Box<dyn FnOnce(&View, Viewport, &C) -> Result<(), E>>;
type Input<E> = Box<dyn FnOnce(&HostInput, SurfaceId) -> Result<(), E>>;

struct Step<C, L, E> {
    start: Start,
    work: Work<C, L, E>,
}

enum Work<C, L, E> {
    Act(Action<C, E>),
    Check(Check<C, E>),
    Input(Input<E>),
    Mark(L, MarkRecording),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum MarkRecording {
    FirstFrame,
    EveryFrame,
}

#[derive(Clone, Copy, Debug)]
struct Marked {
    surface: SurfaceId,
    time: FrameTime,
}

struct Playing<C, L, E> {
    context: Rc<C>,
    waiting: VecDeque<Step<C, L, E>>,
    phase: Phase<E>,
    started: Vec<(Marked, L)>,
    recording: Option<MarkRecording>,
    previous: Option<Duration>,
    frame_requested: bool,
    links: PacedLinks,
}

enum Phase<E> {
    Ready,
    Queued,
    Failed(E),
}

impl<C, L, E> Steps<C, L, E> {
    #[must_use]
    pub fn new(context: C) -> Self {
        Self {
            context: Rc::new(context),
            waiting: VecDeque::new(),
            links: PacedLinks::default(),
        }
    }

    fn add(&mut self, start: Start, work: Work<C, L, E>) {
        self.waiting.push_back(Step { start, work });
    }

    fn into_playing(self) -> Playing<C, L, E> {
        Playing {
            context: self.context,
            waiting: self.waiting,
            phase: Phase::Ready,
            started: Vec::new(),
            recording: None,
            previous: None,
            frame_requested: false,
            links: self.links,
        }
    }
}

impl<C: 'static, L: Clone + 'static, E: From<Error> + 'static> Steps<C, L, E> {
    pub fn push(
        &mut self,
        start: Start,
        action: impl FnOnce(&mut View, Viewport, &C) -> Result<(), E> + 'static,
    ) {
        self.add(start, Work::Act(Box::new(action)));
    }

    pub fn push_at_rest_and_mark(
        &mut self,
        label: L,
        action: impl FnOnce(&mut View, Viewport, &C) -> Result<(), E> + 'static,
    ) {
        self.push(Start::at_rest(), action);
        self.mark(Start::NEXT, label);
    }

    pub fn check(
        &mut self,
        start: Start,
        check: impl FnOnce(&View, Viewport, &C) -> Result<(), E> + 'static,
    ) {
        self.add(start, Work::Check(Box::new(check)));
    }

    pub fn press(&mut self, start: Start, keys: impl IntoIterator<Item = Key>) {
        let keys: Vec<_> = keys.into_iter().collect();
        let press = move |input: &HostInput, surface| {
            input
                .press(surface, &keys)
                .map_err(|failure| Error::Press(failure).into())
        };
        self.add(start, Work::Input(Box::new(press)));
    }

    pub fn mark(&mut self, start: Start, label: L) {
        self.add(start, Work::Mark(label, MarkRecording::FirstFrame));
    }

    pub fn mark_sequence(&mut self, start: Start, label: L) {
        self.add(start, Work::Mark(label, MarkRecording::EveryFrame));
    }

    pub fn record(self, app: App, script: HostScript) -> Result<Vec<(L, RecordedFrame)>, E> {
        let playing = Rc::new(RefCell::new(self.into_playing()));
        let actions = app.view_actions();
        let (observed, ending) = (Rc::clone(&playing), Rc::clone(&playing));
        let app = app
            .view_observer(move |surface, time, viewport, view| {
                let shown = Marked { surface, time };
                observe(&observed, &actions, shown, viewport, view);
            })
            .exit_when(move || matches!(ending.borrow().phase, Phase::Failed(_)));
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

#[must_use]
pub fn at_rest(view: &View) -> bool {
    is_at_rest(view.needs_visual_frame(), view.next_deadline())
}

impl Start {
    pub const NEXT: Self = Self {
        delay: None,
        every: None,
        condition: None,
    };

    #[must_use]
    pub fn at_rest() -> Self {
        Self {
            condition: Some(Box::new(at_rest)),
            ..Self::NEXT
        }
    }

    #[must_use]
    pub const fn after(delay: Duration) -> Self {
        Self {
            delay: Some(delay),
            every: None,
            condition: None,
        }
    }

    #[must_use]
    pub fn when(condition: impl Fn(&View) -> bool + 'static) -> Self {
        Self {
            condition: Some(Box::new(condition)),
            ..Self::NEXT
        }
    }

    #[must_use]
    pub fn sampled(every: FrameInterval, condition: impl Fn(&View) -> bool + 'static) -> Self {
        Self {
            every: Some(every),
            ..Self::when(condition)
        }
    }

    fn due(&self, previous: Option<Duration>) -> Option<Duration> {
        (previous.zip(self.delay)).map(|(previous, delay)| previous.saturating_add(delay))
    }

    fn starts(&self, view: &View, now: Duration, previous: Option<Duration>) -> bool {
        self.due(previous).is_none_or(|due| now >= due)
            && (self.condition.as_ref()).is_none_or(|holds| holds(view))
    }
}

impl<C, L, E> Playing<C, L, E> {
    fn take_started(&mut self, view: &View, now: Duration) -> Option<Work<C, L, E>> {
        let next = self.waiting.front()?;
        if !(matches!(self.phase, Phase::Ready) && next.start.starts(view, now, self.previous)) {
            return None;
        }
        self.waiting.pop_front().map(|step| step.work)
    }

    fn waits_for(&self) -> Pace {
        match (&self.phase, self.waiting.front()) {
            (Phase::Queued, _) => Pace::FrameAtSameTime,
            (Phase::Failed(_), _) | (Phase::Ready, None) => Pace::Finished,
            (Phase::Ready, Some(next)) => Pace::Wake {
                at: next.start.due(self.previous),
                every: next.start.every,
            },
        }
    }
}

impl Marked {
    fn order(&self, frame: &RecordedFrame) -> Ordering {
        if frame.surface == self.surface {
            self.time.sequence.cmp(&frame.time.sequence)
        } else {
            (self.time.presentation).cmp(&frame.time.presentation)
        }
    }
}

fn label<L: Clone>(frames: Vec<RecordedFrame>, started: &[(Marked, L)]) -> Vec<(L, RecordedFrame)> {
    let step = |frame: &RecordedFrame| {
        (started.iter()).rposition(|(marked, _)| marked.order(frame).is_le())
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

fn on_screen(frames: &[RecordedFrame], marked: Marked) -> impl Iterator<Item = &RecordedFrame> {
    (frames.iter())
        .filter(move |frame| marked.order(frame).is_ge())
        .map(|frame| (frame.surface, frame))
        .collect::<BTreeMap<_, _>>()
        .into_values()
}

#[cfg(test)]
mod tests;

fn observe<C: 'static, L: 'static, E: 'static>(
    playing: &Rc<RefCell<Playing<C, L, E>>>,
    actions: &ViewActions,
    shown: Marked,
    viewport: Viewport,
    view: &View,
) {
    let now = view.motion().now();
    let mut state = playing.borrow_mut();
    let mut marked = false;
    while let Some(work) = state.take_started(view, now) {
        match work {
            Work::Mark(label, recording) => {
                state.started.push((shown, label));
                state.previous = Some(now);
                state.recording = Some(recording);
                marked = true;
            }
            Work::Check(check) => {
                state.previous = Some(now);
                if let Err(failure) = check(view, viewport, &state.context) {
                    state.phase = Phase::Failed(failure);
                }
            }
            Work::Act(action) => {
                let act = move |view: &mut View, context: &C| action(view, viewport, context);
                queue(playing, &mut state, actions, shown.surface, act);
            }
            Work::Input(input) => {
                let reported = input(&state.links.input, shown.surface);
                queue(playing, &mut state, actions, shown.surface, |_, _| reported);
            }
        }
    }
    if marked || state.recording == Some(MarkRecording::EveryFrame) {
        state.links.record_requests.request();
    }
    let waits = state.waits_for();
    if matches!(waits, Pace::Wake { at: Some(at), .. } if at > now)
        && !mem::replace(&mut state.frame_requested, true)
    {
        let requested = Rc::clone(playing);
        actions.dispatch(shown.surface, move |_| {
            requested.borrow_mut().frame_requested = false;
        });
    }
    state.links.pace.publish(waits);
}

fn queue<C: 'static, L: 'static, E: 'static>(
    playing: &Rc<RefCell<Playing<C, L, E>>>,
    state: &mut Playing<C, L, E>,
    actions: &ViewActions,
    surface: SurfaceId,
    run: impl FnOnce(&mut View, &C) -> Result<(), E> + 'static,
) {
    state.phase = Phase::Queued;
    let (playing, context) = (Rc::clone(playing), Rc::clone(&state.context));
    actions.dispatch(surface, move |view| {
        let ran = run(view, &context);
        let mut state = playing.borrow_mut();
        state.previous = Some(view.motion().now());
        state.phase = match ran {
            Ok(()) => Phase::Ready,
            Err(failure) => Phase::Failed(failure),
        };
        state.links.pace.publish(state.waits_for());
    });
}
