// SPDX-License-Identifier: GPL-3.0-only
mod back_gesture;
mod outlines;

use crate::Error;
pub use back_gesture::{BackGesture, BackMotion, Page};
use lavashell::prelude::*;
use lavashell_example_support::{Theme, ThemeChoice, icon, lucide};
pub use outlines::Shape;
use std::{cell::RefCell, rc::Rc, time::Duration};

pub const TIMELINE: [(Duration, f64); 4] = [
    (Duration::ZERO, 0.0),
    (Duration::from_millis(700), 0.26),
    (Duration::from_millis(1450), 0.73),
    (Duration::from_millis(2200), 1.0),
];

const AUTOPLAY_TARGETS: [f64; 6] = [0.85, 0.17, 0.68, 0.32, 0.91, 0.08];
const AUTOPLAY_EXPANSIONS: [bool; 4] = [true, true, false, false];

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
    pub render_cost: State<Duration>,
    pub back: BackGesture,
    pub icons: HeaderIcons,
    pub theme: Theme,
    morph: Rc<RefCell<Option<Morph>>>,
    star_selected: State<bool>,
    autoplay_step: State<usize>,
    target_spring: Spring,
    timeline_frames: Keyframes<f64>,
}

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
    pub fn new(autoplay: bool, theme: ThemeChoice) -> Result<Self, Error> {
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
            render_cost: State::new(Duration::ZERO),
            back: BackGesture::new().map_err(Error::Motion)?,
            icons: HeaderIcons::decode()?,
            theme: Theme::new(theme)?,
            morph: Rc::default(),
            star_selected: State::new(false),
            autoplay_step: State::new(0),
            target_spring,
            timeline_frames,
        };
        demo.restart().map_err(Error::Motion)?;
        Ok(demo)
    }

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

    fn sampled(&self) -> Option<Animation<Shape>> {
        self.morph
            .borrow()
            .as_ref()
            .map(|morph| morph.shape.clone())
    }

    #[must_use]
    pub fn shape(&self) -> Option<Shape> {
        self.sampled().map(|shape| shape.get())
    }

    #[must_use]
    pub fn morph_finished(&self) -> bool {
        self.sampled().is_none_or(|shape| shape.is_finished())
    }

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

    #[must_use]
    pub fn timeline_length(&self) -> Duration {
        self.timeline_frames.duration()
    }

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
        self.back.settle()?;
        if let Some(shape) = self.sampled() {
            shape.snap(shape.target())?;
        }
        self.timeline.snap(self.timeline.target())
    }

    pub fn move_target(&self, value: f64) -> Result<(), AnimationError> {
        self.target.set_if_changed(value);
        self.spring.animate_to(value, self.spec(self.target_spring))
    }

    pub fn change_shape(&self) -> Result<(), AnimationError> {
        let star = !self.star_selected.get();
        self.star_selected.set(star);
        let sampled =
            (self.morph.borrow().as_ref()).map(|morph| (morph.shape.clone(), morph.target(star)));
        sampled.map_or(Ok(()), |(shape, target)| {
            shape.animate_to(target, self.spec(Tween::new(Duration::from_millis(760))))
        })
    }

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
