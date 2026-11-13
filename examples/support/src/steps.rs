// SPDX-License-Identifier: GPL-3.0-only
use crate::Error;
use crate::frame_log::record;
#[cfg(any(test, feature = "testing"))]
use crate::testing::{KeyboardKey, press};
use lavashell::core::frame::FrameTime;
use lavashell::core::surface::SurfaceId;
use lavashell::prelude::*;
use lavashell_test_platform::{
    FrameInterval, HostScript, Pace, RecordedFrame, StepsPace, is_at_rest,
};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::{cell::RefCell, cmp::Ordering, mem, rc::Rc, time::Duration};

/// Steps played by an app on its views, one after another. Each step has a start that says in which frame it runs.
/// A step acts on a view, or marks the frame that started it with a label. Each action and check receives the
/// viewport of the view and the context `C` of the steps. The steps publish what they wait for to their pace.
pub struct Steps<C, L, E> {
    context: Rc<C>,
    waiting: VecDeque<Step<C, L, E>>,
    pub(crate) pace: Pace,
}

/// In which frame a step runs: the first frame after the step before in which the delay has passed and the condition
/// holds.
pub struct Start {
    /// The time from the start of the step before until the step can run.
    pub(crate) delay: Option<Duration>,
    /// The time between the frames that a test platform makes while the step waits, for a condition that a motion
    /// without deadlines changes.
    pub(crate) every: Option<FrameInterval>,
    pub(crate) condition: Option<Condition>,
}

pub(crate) type Condition = Box<dyn Fn(&View) -> bool>;

type Action<C, E> = Box<dyn FnOnce(&mut View, Viewport, &C) -> Result<(), E>>;
#[cfg(any(test, feature = "testing"))]
type Check<C, E> = Box<dyn FnOnce(&View, Viewport, &C) -> Result<(), E>>;

struct Step<C, L, E> {
    start: Start,
    work: Work<C, L, E>,
}

pub(crate) enum Work<C, L, E> {
    /// Changes the view after the frame that started the step.
    Act(Action<C, E>),
    /// Labels the frame that started the step, and each later frame until the next mark.
    Mark(L),
    /// Reads the view in the frame that started the step.
    #[cfg(any(test, feature = "testing"))]
    Check(Check<C, E>),
    /// Presses and releases each key on a keyboard of the host, whose focus is on the view of the frame that started
    /// the step.
    #[cfg(any(test, feature = "testing"))]
    Press(Vec<KeyboardKey>),
}

/// The frame that started a mark.
#[derive(Clone, Copy, Debug)]
struct Marked {
    surface: SurfaceId,
    time: FrameTime,
}

/// The steps of an app while it runs.
struct Playing<C, L, E> {
    context: Rc<C>,
    waiting: VecDeque<Step<C, L, E>>,
    phase: Phase<E>,
    /// The frame that started each mark, with the label of the mark.
    started: Vec<(Marked, L)>,
    /// The time at which the step that ran last started.
    previous: Option<Duration>,
    /// Whether an empty action waits to give a view a frame for the next step, whose delay has not passed yet.
    frame_requested: bool,
    pace: Pace,
}

/// How far the next step got.
enum Phase<E> {
    /// The next step runs once a frame shows a view as its start describes.
    Ready,
    /// The next step acts on a view and has not run yet.
    Queued,
    /// A step failed, so no later step runs.
    Failed(E),
}

impl<C, L, E> Steps<C, L, E> {
    #[must_use]
    pub fn new(context: C) -> Self {
        Self {
            context: Rc::new(context),
            waiting: VecDeque::new(),
            pace: Pace::default(),
        }
    }

    pub(crate) fn add(&mut self, start: Start, work: Work<C, L, E>) {
        self.waiting.push_back(Step { start, work });
    }

    fn into_playing(self) -> Playing<C, L, E> {
        Playing {
            context: self.context,
            waiting: self.waiting,
            phase: Phase::Ready,
            started: Vec::new(),
            previous: None,
            frame_requested: false,
            pace: self.pace,
        }
    }
}

impl<C: 'static, L: Clone + 'static, E: From<Error> + 'static> Steps<C, L, E> {
    /// Adds `action` as the last step, which acts on a view after the frame that `start` selects.
    pub fn push(
        &mut self,
        start: Start,
        action: impl FnOnce(&mut View, Viewport, &C) -> Result<(), E> + 'static,
    ) {
        self.add(start, Work::Act(Box::new(action)));
    }

    /// Adds a mark as the last step, which labels the frame that `start` selects, and each later frame until the next
    /// mark, with `label`.
    pub fn mark(&mut self, start: Start, label: L) {
        self.add(start, Work::Mark(label));
    }

    /// Records `app` on a test platform that plays `script`, while the steps play on its views. Returns each recorded
    /// frame from the first mark on, with the label of the newest mark at its presentation. Replaces the view
    /// observer of `app`, and adds an exit predicate that ends the run once a step failed.
    ///
    /// # Errors
    /// Returns the failure of the first step that failed, an error when the recording fails, and
    /// [`Error::UnplayedSteps`] when the recording came to rest before every step ran.
    pub fn record(self, app: App, script: HostScript) -> Result<Vec<(L, RecordedFrame)>, E> {
        let playing = Rc::new(RefCell::new(self.into_playing()));
        let actions = app.view_actions();
        let (observed, ending) = (Rc::clone(&playing), Rc::clone(&playing));
        let app = app
            .view_observer(move |surface, timing, viewport, view| {
                let shown = Marked {
                    surface,
                    time: timing,
                };
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

/// Whether `view` is at rest: it does not need a visual frame and does not wait for a deadline.
#[must_use]
pub fn at_rest(view: &View) -> bool {
    is_at_rest(view.needs_visual_frame(), view.next_deadline())
}

impl Start {
    /// In the frame in which the step before ran, or in the frame that shows its action.
    pub const NEXT: Self = Self {
        delay: None,
        every: None,
        condition: None,
    };

    /// In a frame that shows a view at rest, as [`at_rest`] describes.
    #[must_use]
    pub fn at_rest() -> Self {
        Self {
            condition: Some(Box::new(at_rest)),
            ..Self::NEXT
        }
    }

    /// The time from which the step can run, when the step before started at `previous`.
    fn due(&self, previous: Option<Duration>) -> Option<Duration> {
        (previous.zip(self.delay)).map(|(previous, delay)| previous.saturating_add(delay))
    }

    /// Whether a frame that shows `view` at `now` starts the step, when the step before started at `previous`.
    fn starts(&self, view: &View, now: Duration, previous: Option<Duration>) -> bool {
        self.due(previous).is_none_or(|due| now >= due)
            && (self.condition.as_ref()).is_none_or(|holds| holds(view))
    }
}

impl<C, L, E> Playing<C, L, E> {
    /// Takes the work of the next step when a frame that shows `view` at `now` starts it.
    fn take_started(&mut self, view: &View, now: Duration) -> Option<Work<C, L, E>> {
        let next = self.waiting.front()?;
        if !(matches!(self.phase, Phase::Ready) && next.start.starts(view, now, self.previous)) {
            return None;
        }
        self.waiting.pop_front().map(|step| step.work)
    }

    /// What the steps wait for.
    fn waits_for(&self) -> StepsPace {
        match (&self.phase, self.waiting.front()) {
            (Phase::Queued, _) => StepsPace::Queued,
            (Phase::Failed(_), _) | (Phase::Ready, None) => StepsPace::Ended,
            (Phase::Ready, Some(next)) => StepsPace::Waiting {
                due: next.start.due(self.previous),
                every: next.start.every,
            },
        }
    }
}

impl Marked {
    /// How the frame that started the mark is ordered against `frame`: by sequence on the same surface, and by
    /// presentation time on another surface.
    fn order(&self, frame: &RecordedFrame) -> Ordering {
        if frame.surface == self.surface {
            self.time.sequence.cmp(&frame.time.sequence)
        } else {
            (self.time.presentation).cmp(&frame.time.presentation)
        }
    }
}

/// Labels each frame with the newest mark started by it or by an earlier frame. A mark during which no later frame
/// was recorded is shown by the newest frame of each surface up to the frame that started it.
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

/// The newest frame of each surface up to `marked`, in the order of the surfaces.
fn on_screen(frames: &[RecordedFrame], marked: Marked) -> impl Iterator<Item = &RecordedFrame> {
    (frames.iter())
        .filter(move |frame| marked.order(frame).is_ge())
        .map(|frame| (frame.surface, frame))
        .collect::<BTreeMap<_, _>>()
        .into_values()
}

#[cfg(test)]
mod tests;

/// Runs each next step started by `shown`, the frame that shows `view` in `viewport`, and publishes what the steps
/// wait for then. A check or a mark runs at once. An action, or a press of keys, is queued on `view`, and later steps
/// wait until it has run. While the delay of the next step has not passed, an empty action waits on `view`, so the
/// view gets a frame at the next present, which a test platform makes at the time the steps wait for.
fn observe<C: 'static, L: 'static, E: 'static>(
    playing: &Rc<RefCell<Playing<C, L, E>>>,
    actions: &ViewActions,
    shown: Marked,
    viewport: Viewport,
    view: &View,
) {
    let now = view.motion().now();
    let mut state = playing.borrow_mut();
    while let Some(work) = state.take_started(view, now) {
        match work {
            Work::Mark(label) => {
                state.started.push((shown, label));
                state.previous = Some(now);
            }
            #[cfg(any(test, feature = "testing"))]
            Work::Check(check) => {
                state.previous = Some(now);
                if let Err(failure) = check(view, viewport, &state.context) {
                    state.phase = Phase::Failed(failure);
                }
            }
            Work::Act(action) => queue(playing, &mut state, actions, (shown, viewport), action),
            #[cfg(any(test, feature = "testing"))]
            Work::Press(keys) => {
                press(&state.pace, shown.surface, &keys);
                queue(
                    playing,
                    &mut state,
                    actions,
                    (shown, viewport),
                    Box::new(|_, _, _| Ok(())),
                );
            }
        }
    }
    if matches!(state.waits_for(), StepsPace::Waiting { due: Some(due), .. } if due > now)
        && !mem::replace(&mut state.frame_requested, true)
    {
        let requested = Rc::clone(playing);
        actions.dispatch(shown.surface, move |_| {
            requested.borrow_mut().frame_requested = false;
        });
    }
    state.pace.publish(state.waits_for());
}

/// Queues `action` on the view of the frame `shown` in `viewport`. The next step waits until the action has run.
fn queue<C: 'static, L: 'static, E: 'static>(
    playing: &Rc<RefCell<Playing<C, L, E>>>,
    state: &mut Playing<C, L, E>,
    actions: &ViewActions,
    (shown, viewport): (Marked, Viewport),
    action: Action<C, E>,
) {
    state.phase = Phase::Queued;
    let (playing, context) = (Rc::clone(playing), Rc::clone(&state.context));
    actions.dispatch(shown.surface, move |view| {
        let ran = action(view, viewport, &context);
        let mut state = playing.borrow_mut();
        state.previous = Some(view.motion().now());
        state.phase = match ran {
            Ok(()) => Phase::Ready,
            Err(failure) => Phase::Failed(failure),
        };
        state.pace.publish(state.waits_for());
    });
}
