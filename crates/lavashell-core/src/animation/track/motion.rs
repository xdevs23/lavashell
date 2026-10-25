// SPDX-License-Identifier: GPL-3.0-only
use super::{
    Advanced, Animatable, Animation, AnimationError, Cell, Duration, Program, Rc, RefCell,
    Scheduled, TimerState, Weak,
};
use crate::geometry::Viewport;
use crate::units::Dp;

pub(super) struct Registry {
    pub(super) tracks: RefCell<Vec<Weak<dyn Advanced>>>,
    pub(super) timers: RefCell<Vec<Weak<TimerState>>>,
    pub(super) advances: Cell<u64>,
    pub(super) now: Cell<Duration>,
    pub(super) reduced: Cell<bool>,
    pub(super) source: RefCell<Option<Rc<dyn Fn() -> Duration>>>,
    /// The device pixel size of the view. A motion ends once it is within the visible step for this size of its
    /// target.
    pub(super) pixel: Cell<Dp>,
    skipped_rest: Cell<Option<Duration>>,
}

impl Default for Registry {
    fn default() -> Self {
        Self {
            tracks: RefCell::default(),
            timers: RefCell::default(),
            advances: Cell::default(),
            now: Cell::default(),
            reduced: Cell::default(),
            source: RefCell::default(),
            pixel: Cell::new(default_pixel()),
            skipped_rest: Cell::default(),
        }
    }
}

/// The device pixel size of the default viewport, used until the first frame of a view sets the size.
pub(super) fn default_pixel() -> Dp {
    Viewport::default().pixel()
}

#[derive(Clone, Default)]
pub struct Motion(pub(super) Rc<Registry>);

/// The error returned when an animation cannot be bound to a `Motion`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum BindError {
    #[error("the animation is bound to another Motion host; share its target between views")]
    OtherMotion,
}

/// A value that moves on the clock of the `Motion` that it is bound to, such as an animation.
pub trait Bindable {
    /// # Errors
    ///
    /// Fails when the value is bound to another `Motion` that is still alive.
    fn bind(&self, motion: &Motion) -> Result<(), BindError>;
}

impl<T: Animatable> Bindable for Animation<T> {
    fn bind(&self, motion: &Motion) -> Result<(), BindError> {
        motion.bind(self)
    }
}

impl Motion {
    /// Sets the function that returns the current time on the monotonic clock used by `advance()`.
    /// Every change to a bound animation first advances the animation to that time, so an animation
    /// started or resumed after the host was idle does not jump ahead. A host that advances the
    /// clock by hand needs no time source.
    pub fn time_source(&self, source: impl Fn() -> Duration + 'static) {
        *self.0.source.borrow_mut() = Some(Rc::new(source));
    }

    /// A new animation at `value`, bound to this clock.
    ///
    /// # Errors
    ///
    /// Fails when a part of `value` is not finite.
    pub fn animation<T: Animatable>(&self, value: T) -> Result<Animation<T>, AnimationError> {
        let animation = Animation::new(value)?;
        self.attach(&animation);
        Ok(animation)
    }

    /// Binds `animation` to this clock. An animation has one track, which two surfaces with
    /// independent frame pacing must not sample. Two views share a target value, each with its own
    /// animation.
    ///
    /// # Errors
    ///
    /// Fails when the animation is bound to another `Motion` that is still alive.
    pub fn bind<T: Animatable>(&self, animation: &Animation<T>) -> Result<(), BindError> {
        if let Some(existing) = animation.0.owner.borrow().upgrade() {
            return if Rc::ptr_eq(&existing, &self.0) {
                Ok(())
            } else {
                Err(BindError::OtherMotion)
            };
        }
        self.attach(animation);
        Ok(())
    }

    /// Moves `animation`, which is bound to no living `Motion`, onto this clock.
    fn attach<T: Animatable>(&self, animation: &Animation<T>) {
        // A track can move to a new host after its old host is dropped. The timing starts again at
        // the time of the new clock, with the same elapsed time and pause state.
        {
            let mut track = animation.0.track.borrow_mut();
            track.timing.offset = track.elapsed();
            track.now = self.0.now.get();
            track.timing.start = track.now;
            track.timing.paused = track.timing.paused.map(|_| track.now);
        }
        let inner: Rc<dyn Advanced> = animation.0.clone();
        inner.remeasure(self.0.pixel.get());
        inner.advance(self.0.now.get(), self.0.reduced.get());
        self.0.tracks.borrow_mut().push(Rc::downgrade(&inner));
        *animation.0.owner.borrow_mut() = Rc::downgrade(&self.0);
        animation.0.revise();
    }

    /// Samples every track at `time`, then runs the timers due by then.
    pub fn advance(&self, time: Duration) {
        self.advance_tracks(time);
        self.fire_timers();
    }

    fn advance_tracks(&self, time: Duration) {
        self.0.now.set(time.max(self.0.now.get()));
        self.0.tracks.borrow_mut().retain(|weak| {
            if let Some(track) = weak.upgrade() {
                track.advance(self.0.now.get(), self.0.reduced.get());
                true
            } else {
                false
            }
        });
    }

    pub fn is_active(&self) -> bool {
        self.0
            .tracks
            .borrow()
            .iter()
            .filter_map(Weak::upgrade)
            .any(|track| track.active())
    }

    pub(crate) fn moves_read_values(&self) -> bool {
        (self.0.tracks.borrow().iter())
            .filter_map(Weak::upgrade)
            .any(|track| track.active() && track.read())
    }

    pub(crate) fn next_rest(&self) -> Option<Duration> {
        (self.0.tracks.borrow().iter())
            .filter_map(Weak::upgrade)
            .filter(|track| track.active())
            .filter_map(|track| track.rests_at())
            .min()
    }

    pub(crate) fn rest_due(&self) -> bool {
        self.next_rest().is_some_and(|rest| rest <= self.current())
    }

    /// Sets the device pixel size to `pixel` and computes the visible step of every motion again,
    /// running motions included.
    pub(crate) fn set_device_pixel(&self, pixel: Dp) {
        if self.0.pixel.replace(pixel) == pixel {
            return;
        }
        for track in self.0.tracks.borrow().iter().filter_map(Weak::upgrade) {
            track.remeasure(pixel);
        }
    }

    /// Sets reduced motion. While `enabled` is true, running tracks jump to their targets and later
    /// targets apply at once.
    pub(crate) fn reduced_motion(&self, enabled: bool) {
        self.0.reduced.set(enabled);
        self.advance_tracks(self.0.now.get());
    }

    #[must_use]
    pub fn now(&self) -> Duration {
        self.0.now.get()
    }

    /// The latest time, on the epoch of `advance`, at which a motion that reduced motion applied at once would
    /// have come to rest.
    #[must_use]
    pub fn skipped_rest(&self) -> Option<Duration> {
        self.0.skipped_rest.get()
    }
}

impl<T: Animatable> Animation<T> {
    /// Rests at `target` at once and records on the clock of the animation when `program` would have come to rest.
    pub(super) fn skip_to(&self, target: T, program: Program<T>) {
        self.rest_at(target);
        let Some(owner) = self.0.owner.borrow().upgrade() else {
            return;
        };
        let rest = Scheduled::new(program).rest();
        if let Some(rests) = rest.and_then(|rest| self.0.track.borrow().timing.reaching(rest)) {
            owner
                .skipped_rest
                .set(owner.skipped_rest.get().max(Some(rests)));
        }
    }
}
