// SPDX-License-Identifier: GPL-3.0-only
use super::program::{Sample, Scheduled};
use super::{
    Animatable, Direction, Keyframes, MotionSpec, Playback, Playing, Program, Repeat, Timing,
    Trajectory, VectorSpace,
};
use crate::reactive::State;
use crate::units::Dp;
use std::{
    cell::{Cell, RefCell, RefMut},
    rc::{Rc, Weak},
    time::Duration,
};

struct Track<T: Animatable> {
    target: T,
    /// The target, with every part that only the value at the start of the motion has added at zero
    /// strength. `trajectory` is in the vector space of this value.
    end_value: T,
    trajectory: Trajectory<T::Vector>,
    program: Option<Scheduled<T>>,
    timing: Timing,
    now: Duration,
    finished: bool,
}

impl<T: Animatable> Track<T> {
    fn run(&mut self, program: Program<T>) {
        self.program = Some(Scheduled::new(program));
        self.finished = false;
    }

    /// Computes the visible step of a running motion toward a target again for the device pixel size `pixel`.
    /// Returns whether the step changed, which also changes the schedule of the motion.
    fn remeasure(&mut self, pixel: Dp) -> bool {
        let step = T::visible_step(pixel);
        let Some(Program::Target {
            from,
            to,
            velocity,
            spec,
            step: measured,
        }) = self.program.as_ref().map(Scheduled::program)
        else {
            return false;
        };
        if self.finished || measured.total_cmp(&step).is_eq() {
            return false;
        }
        let program = Program::Target {
            from: from.clone(),
            to: to.clone(),
            velocity: velocity.clone(),
            spec: *spec,
            step,
        };
        self.run(program);
        true
    }

    fn elapsed(&self) -> Duration {
        if self.finished {
            return self.timing.offset;
        }
        self.timing.elapsed(self.now)
    }

    /// The current value of the track. A finished track returns the target as given, without the
    /// parts that only the value at the start of the motion had.
    fn value(&self) -> T {
        if self.finished {
            self.target.clone()
        } else {
            self.end_value.at(self.trajectory.position.clone())
        }
    }

    fn playing(&self) -> Option<Playing<T>> {
        if self.finished || self.timing.paused.is_some() {
            return None;
        }
        let scheduled = self.program.clone()?;
        // While keyframes play, `end_value` changes with each segment. `target` is the same for the whole motion.
        let end = match scheduled.program() {
            Program::Target { .. } => self.end_value.clone(),
            Program::Frames { .. } => self.target.clone(),
        };
        Some(Playing::scheduled(end, scheduled, self.timing))
    }

    fn resting(value: T, now: Duration, rate: f64) -> Self {
        Self {
            trajectory: Trajectory::at_rest(value.vector()),
            end_value: value.clone(),
            target: value,
            program: None,
            timing: Timing {
                start: now,
                paused: None,
                offset: Duration::ZERO,
                rate,
            },
            now,
            finished: true,
        }
    }

    /// Stops at `value` with the same clock and playback rate.
    fn rest(&mut self, value: T) {
        *self = Self::resting(value, self.now, self.timing.rate);
    }

    /// Sets a finished track to rest at its target, in the vector space of the target, so the next
    /// motion starts from the target. Before this call, a seek samples the finished motion exactly.
    fn settle(&mut self) {
        if self.finished {
            self.rest(self.target.clone());
        }
    }

    fn sample(&mut self) {
        if self.finished {
            return;
        }
        let elapsed = self.elapsed();
        let Some(scheduled) = &self.program else {
            return;
        };
        let Sample {
            frame,
            trajectory,
            ended,
        } = scheduled.sample(elapsed);
        if let Some(frame) = frame {
            self.end_value = frame;
        }
        self.trajectory = trajectory;
        if let Some(ended) = ended {
            self.timing.offset = ended;
            self.finished = true;
        }
    }
}

struct Inner<T: Animatable> {
    value: State<T>,
    status: State<Status>,
    revision: State<u64>,
    playing: RefCell<CachedPlaying<T>>,
    track: RefCell<Track<T>>,
    reduced: Cell<bool>,
    owner: RefCell<Weak<Registry>>,
}

struct CachedPlaying<T: Animatable> {
    revision: u64,
    playing: Option<Rc<Playing<T>>>,
}

impl<T: Animatable> CachedPlaying<T> {
    fn get_or_build(
        &mut self,
        revision: u64,
        build: impl FnOnce() -> Option<Playing<T>>,
    ) -> Option<Rc<Playing<T>>> {
        if self.revision != revision {
            *self = Self {
                revision,
                playing: build().map(Rc::new),
            };
        }
        self.playing.clone()
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Status {
    Running,
    Paused,
    Finished,
}

impl<T: Animatable> Inner<T> {
    fn publish(&self, track: RefMut<'_, Track<T>>) {
        let value = track.value();
        drop(track);
        self.publish_status();
        self.value.set_if_changed(value);
    }

    fn publish_status(&self) {
        let track = self.track.borrow();
        let status = if track.finished || track.program.is_none() {
            Status::Finished
        } else if track.timing.paused.is_some() {
            Status::Paused
        } else {
            Status::Running
        };
        drop(track);
        self.status.set_if_changed(status);
    }

    fn revise(&self) {
        self.revision.update(|revision| *revision += 1);
    }

    fn synchronize(&self) {
        let Some(owner) = self.owner.borrow().upgrade() else {
            return;
        };
        let source = owner.source.borrow().clone();
        if let Some(source) = source {
            self.advance(source(), owner.reduced.get());
        }
    }

    /// The device pixel size of the view that advances this track, or of the default viewport before a view
    /// advances it.
    fn pixel(&self) -> Dp {
        let owner = self.owner.borrow().upgrade();
        owner.map_or_else(motion::default_pixel, |owner| owner.pixel.get())
    }
}

trait Advanced {
    fn advance(&self, time: Duration, reduced: bool);

    fn active(&self) -> bool;

    fn read(&self) -> bool;

    fn rests_at(&self) -> Option<Duration>;

    fn remeasure(&self, pixel: Dp);
}

impl<T: Animatable> Advanced for Inner<T> {
    fn advance(&self, time: Duration, reduced: bool) {
        self.reduced.set(reduced);
        let mut track = self.track.borrow_mut();
        track.now = time.max(track.now);
        let finished = track.finished;
        if reduced && track.program.is_some() {
            let target = track.target.clone();
            track.rest(target);
        } else {
            track.sample();
        }
        if finished {
            return;
        }
        let rests = track.finished;
        self.publish(track);
        if rests {
            self.revise();
        }
    }

    fn active(&self) -> bool {
        let t = self.track.borrow();
        t.program.is_some() && !t.finished && t.timing.paused.is_none()
    }

    fn read(&self) -> bool {
        self.value.is_read()
    }

    fn rests_at(&self) -> Option<Duration> {
        let track = self.track.borrow();
        if track.finished || track.program.is_none() {
            return None;
        }
        track.timing.reaching(track.program.as_ref()?.rest()?)
    }

    fn remeasure(&self, pixel: Dp) {
        if self.track.borrow_mut().remeasure(pixel) {
            self.revise();
        }
    }
}

/// A value that moves toward a target over time and publishes each new value as reactive state. A new target
/// starts the motion from the current position and velocity.
pub struct Animation<T: Animatable>(Rc<Inner<T>>);

impl<T: Animatable> Clone for Animation<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T: Animatable> Animation<T> {
    /// # Errors
    ///
    /// Fails when `value` is not finite.
    pub fn new(value: T) -> Result<Self, AnimationError> {
        finite(&value)?;
        Ok(Self(Rc::new(Inner {
            value: State::new(value.clone()),
            status: State::new(Status::Finished),
            revision: State::new(0),
            playing: RefCell::new(CachedPlaying {
                revision: 0,
                playing: None,
            }),
            track: RefCell::new(Track::resting(value, Duration::ZERO, 1.0)),
            reduced: Cell::new(false),
            owner: RefCell::new(Weak::new()),
        })))
    }

    #[must_use]
    pub fn get(&self) -> T {
        self.0.value.get()
    }

    pub(crate) fn peek(&self) -> T {
        self.0.value.peek()
    }

    /// The playing motion. A reader of this function reruns when the program or its timing changes or the
    /// program comes to rest. It does not rerun when the value moves.
    pub(crate) fn playing(&self) -> Option<Rc<Playing<T>>> {
        let revision = self.0.revision.get();
        (self.0.playing.borrow_mut()).get_or_build(revision, || self.0.track.borrow().playing())
    }

    #[must_use]
    pub fn target(&self) -> T {
        self.0.track.borrow().target.clone()
    }

    #[must_use]
    pub fn velocity(&self) -> T::Vector {
        let t = self.0.track.borrow();
        t.trajectory.velocity.scale(t.timing.rate)
    }

    #[must_use]
    pub fn is_running(&self) -> bool {
        self.0.status.get() == Status::Running
    }

    #[must_use]
    pub fn is_finished(&self) -> bool {
        self.0.status.get() == Status::Finished
    }

    /// # Errors
    ///
    /// Fails when `target` is not finite.
    pub fn animate_to(&self, target: T, spec: impl Into<MotionSpec>) -> Result<(), AnimationError> {
        finite(&target)?;
        self.retarget(target, spec.into(), None);
        Ok(())
    }

    /// Starts a motion toward `target` at the velocity `velocity`, in vector units of the value per second.
    ///
    /// # Errors
    ///
    /// Fails when `target` or `velocity` is not finite.
    pub fn animate_to_with_velocity(
        &self,
        target: T,
        velocity: T::Vector,
        spec: impl Into<MotionSpec>,
    ) -> Result<(), AnimationError> {
        finite(&target)?;
        if !velocity.finite() {
            return Err(AnimationError::Velocity);
        }
        self.retarget(target, spec.into(), Some(velocity));
        Ok(())
    }

    /// Starts a motion from the current position toward the finite `target`. The motion starts at `velocity`,
    /// or at the current velocity when `velocity` is `None`.
    fn retarget(&self, target: T, spec: MotionSpec, velocity: Option<T::Vector>) {
        self.0.synchronize();
        if spec == MotionSpec::Instant
            || matches!(spec, MotionSpec::Tween(t) if t.duration.is_zero() && t.delay.is_zero())
        {
            self.rest_at(target);
            return;
        }
        let mut t = self.0.track.borrow_mut();
        let same_program = match t.program.as_ref().map(Scheduled::program) {
            Some(Program::Target { spec: old, .. }) => *old == spec,
            _ => false,
        };
        if target == t.target && velocity.is_none() && (same_program || t.finished) {
            return;
        }
        t.sample();
        t.settle();
        let end_value = target.spanning(&t.end_value);
        t.trajectory = end_value.retarget(&t.end_value, t.trajectory.clone());
        t.end_value = end_value;
        if let Some(velocity) = velocity {
            t.trajectory.velocity = velocity.scale(1.0 / t.timing.rate);
        }
        let program = Program::Target {
            from: t.trajectory.position.clone(),
            to: t.trajectory.end.clone(),
            velocity: t.trajectory.velocity.clone(),
            spec,
            step: T::visible_step(self.0.pixel()),
        };
        if self.0.reduced.get() {
            drop(t);
            self.skip_to(target, program);
            return;
        }
        t.run(program);
        t.target = target;
        t.timing.start = t.now;
        t.timing.offset = Duration::ZERO;
        t.timing.paused = None;
        self.0.publish(t);
        self.0.revise();
    }

    pub fn play(&self, frames: Keyframes<T>, playback: Playback) {
        self.0.synchronize();
        let final_reverse = playback.direction == Direction::Alternate
            && matches!(playback.repeat, Repeat::Count(count) if count > 0 && count % 2 == 0);
        let target = if final_reverse {
            frames.first()
        } else {
            frames.last()
        }
        .clone();
        let program = Program::Frames { frames, playback };
        if self.0.reduced.get() {
            self.skip_to(target, program);
            return;
        }
        let mut t = self.0.track.borrow_mut();
        t.rest(target);
        t.run(program);
        t.sample();
        self.0.publish(t);
        self.0.revise();
    }

    /// # Errors
    ///
    /// Fails when `value` is not finite.
    pub fn snap(&self, value: T) -> Result<(), AnimationError> {
        finite(&value)?;
        self.rest_at(value);
        Ok(())
    }

    /// Stops the animation at the finite `value` and publishes that value.
    fn rest_at(&self, value: T) {
        let mut track = self.0.track.borrow_mut();
        let unchanged = track.finished && track.value() == value;
        track.rest(value.clone());
        drop(track);
        self.0.publish_status();
        self.0.value.set_if_changed(value);
        if !unchanged {
            self.0.revise();
        }
    }

    pub fn stop(&self) {
        self.0.synchronize();
        let value = self.get();
        self.rest_at(value);
    }
}

mod error;

pub use error::AnimationError;
pub(crate) use error::finite;

mod motion;

use motion::Registry;
pub use motion::{BindError, Bindable, Motion};

mod timer;

pub use timer::Timer;
use timer::TimerState;

mod rate;

mod timeline;

pub use rate::PlaybackRateError;
