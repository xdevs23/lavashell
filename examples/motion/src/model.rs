// SPDX-License-Identifier: GPL-3.0-only
mod outlines;

use crate::Error;
use lavashell::prelude::*;
use lavashell_example_support::{icon, lucide};
pub use outlines::Shape;
use std::{cell::RefCell, rc::Rc, time::Duration};

/// The keyframes of the timeline demo: each pair is a time and the value reached at that time.
pub const TIMELINE: [(Duration, f64); 4] = [
    (Duration::ZERO, 0.0),
    (Duration::from_millis(700), 0.26),
    (Duration::from_millis(1450), 0.73),
    (Duration::from_millis(2200), 1.0),
];

/// The targets of the spring that autoplay moves to, one after the other.
const AUTOPLAY_TARGETS: [f64; 6] = [0.85, 0.17, 0.68, 0.32, 0.91, 0.08];
/// Whether the card is expanded at each step of autoplay.
const AUTOPLAY_EXPANSIONS: [bool; 4] = [true, true, false, false];

/// The icons of the toggles in the header.
#[derive(Clone, Copy)]
pub struct HeaderIcons {
    pub play: &'static Icon,
    pub pause: &'static Icon,
    pub check: &'static Icon,
    pub minus: &'static Icon,
}

impl HeaderIcons {
    fn decode() -> Result<Self, Error> {
        Ok(Self {
            play: icon(&lucide::PLAY)?,
            pause: icon(&lucide::PAUSE)?,
            check: icon(&lucide::CHECK)?,
            minus: icon(&lucide::MINUS)?,
        })
    }
}

#[derive(Clone)]
pub struct Demo {
    pub expanded: State<bool>,
    pub autoplay: State<bool>,
    pub reduced: State<bool>,
    pub target: State<f64>,
    pub spring: Animation<f64>,
    pub timeline: Animation<f64>,
    pub paused: State<bool>,
    pub time: State<FrameTime>,
    pub render_ms: State<f64>,
    pub icons: HeaderIcons,
    morph: Rc<RefCell<Option<Morph>>>,
    star_selected: State<bool>,
    autoplay_step: State<usize>,
    target_spring: Spring,
    timeline_frames: Keyframes<f64>,
}

/// The animation between the circle and the star outlines, both sampled for the flatness tolerance in `tolerance`.
struct Morph {
    shape: Animation<Shape>,
    tolerance: f64,
    circle: Shape,
    star: Shape,
}

impl Morph {
    fn target(&self, star: bool) -> Shape {
        if star {
            self.star.clone()
        } else {
            self.circle.clone()
        }
    }
}

impl Demo {
    /// # Errors
    /// Returns an error when the keys of the timeline are invalid, an icon does not decode or a
    /// motion cannot take its first value.
    pub fn new(autoplay: bool) -> Result<Self, Error> {
        let target_spring = spring!(Duration::from_millis(720), 0.55);
        let timeline_frames = Keyframes::new(TIMELINE.map(|(at, value)| Keyframe::new(at, value)))
            .map_err(Error::Timeline)?;
        let demo = Self {
            expanded: State::new(false),
            autoplay: State::new(autoplay),
            reduced: State::new(false),
            target: State::new(0.25),
            spring: Animation::new(0.25).map_err(Error::Motion)?,
            timeline: Animation::new(0.0).map_err(Error::Motion)?,
            paused: State::new(false),
            time: State::new(FrameTime::default()),
            render_ms: State::new(0.0),
            icons: HeaderIcons::decode()?,
            morph: Rc::default(),
            star_selected: State::new(false),
            autoplay_step: State::new(0),
            target_spring,
            timeline_frames,
        };
        demo.restart().map_err(Error::Motion)?;
        Ok(demo)
    }

    /// The animation of the morph between the circle and the star. The outlines are sampled so that no
    /// edge is further than `tolerance` from them, as a fraction of the radius of the morph. A tolerance
    /// different from the last one samples the outlines again. The new animation then starts at the
    /// selected shape without motion.
    ///
    /// # Errors
    /// Returns an error when the outlines cannot be sampled for `tolerance` or the morph cannot
    /// start at the shape selected.
    pub fn morph(&self, tolerance: f64) -> Result<Animation<Shape>, Error> {
        let mut morph = self.morph.borrow_mut();
        if let Some(current) = morph.as_ref()
            && current.tolerance.to_bits() == tolerance.to_bits()
        {
            return Ok(current.shape.clone());
        }
        let (circle, star) = outlines::outlines(tolerance)?;
        let shape = Animation::new(if self.star_selected.get() {
            star.clone()
        } else {
            circle.clone()
        })
        .map_err(Error::Motion)?;
        *morph = Some(Morph {
            shape: shape.clone(),
            tolerance,
            circle,
            star,
        });
        Ok(shape)
    }

    /// The animation of the morph, or `None` before the view first requested it through `morph`.
    fn sampled(&self) -> Option<Animation<Shape>> {
        self.morph
            .borrow()
            .as_ref()
            .map(|morph| morph.shape.clone())
    }

    /// The shape shown by the morph, or `None` before the view first requested it through `morph`.
    #[must_use]
    pub fn shape(&self) -> Option<Shape> {
        self.sampled().map(|shape| shape.get())
    }

    /// Whether the morph has come to rest. Before the view first requests it through `morph`, the morph
    /// counts as at rest.
    #[must_use]
    pub fn morph_finished(&self) -> bool {
        self.sampled().is_none_or(|shape| shape.is_finished())
    }

    /// # Errors
    /// Returns an error when a motion of autoplay cannot take its next value.
    pub fn step(&self, frame: FrameTime) -> Result<(), AnimationError> {
        self.time.set(frame);
        if !self.autoplay.get() || self.reduced.get() {
            return Ok(());
        }
        if self.timeline.is_finished() {
            self.restart()?;
        }
        if !(self.spring.is_finished() && self.morph_finished()) {
            return Ok(());
        }
        let step = self.autoplay_step.get();
        self.autoplay_step.set(step.wrapping_add(1));
        self.expanded
            .set_if_changed(AUTOPLAY_EXPANSIONS[step % AUTOPLAY_EXPANSIONS.len()]);
        self.move_target(AUTOPLAY_TARGETS[step % AUTOPLAY_TARGETS.len()])?;
        self.change_shape()
    }

    /// The time from the first to the last key of the timeline.
    #[must_use]
    pub fn timeline_length(&self) -> Duration {
        self.timeline_frames.duration()
    }

    /// # Errors
    /// Returns an error when a motion cannot settle at its target.
    pub fn set_reduced(&self, reduced: bool) -> Result<(), AnimationError> {
        self.reduced.set(reduced);
        if reduced {
            self.settle()?;
        }
        Ok(())
    }

    pub fn spec(&self, spec: impl Into<MotionSpec>) -> MotionSpec {
        if self.reduced.get() {
            MotionSpec::Instant
        } else {
            spec.into()
        }
    }

    fn settle(&self) -> Result<(), AnimationError> {
        self.spring.snap(self.spring.target())?;
        if let Some(shape) = self.sampled() {
            shape.snap(shape.target())?;
        }
        self.timeline.snap(self.timeline.target())
    }

    /// # Errors
    /// Returns an error when `value` is not finite.
    pub fn move_target(&self, value: f64) -> Result<(), AnimationError> {
        self.target.set_if_changed(value);
        self.spring.animate_to(value, self.spec(self.target_spring))
    }

    /// # Errors
    /// Returns an error when the morph cannot move to the other shape.
    pub fn change_shape(&self) -> Result<(), AnimationError> {
        let star = !self.star_selected.get();
        self.star_selected.set(star);
        let sampled =
            (self.morph.borrow().as_ref()).map(|morph| (morph.shape.clone(), morph.target(star)));
        sampled.map_or(Ok(()), |(shape, target)| {
            shape.animate_to(target, self.spec(Tween::new(Duration::from_millis(760))))
        })
    }

    /// # Errors
    /// Returns an error when a motion cannot settle at its target under reduced motion.
    pub fn restart(&self) -> Result<(), AnimationError> {
        self.timeline.play(
            self.timeline_frames.clone(),
            Playback {
                repeat: Repeat::Forever,
                direction: Direction::Alternate,
            },
        );
        self.paused.set(false);
        if self.reduced.get() {
            self.settle()?;
        }
        Ok(())
    }

    pub fn toggle_pause(&self) {
        let paused = !self.paused.get();
        self.paused.set(paused);
        if paused {
            self.timeline.pause();
        } else {
            self.timeline.resume();
        }
    }
}
