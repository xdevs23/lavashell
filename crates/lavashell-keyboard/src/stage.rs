// SPDX-License-Identifier: GPL-3.0-only

mod opening;

use crate::keyboard::Keyboard;
use crate::look;
use lavashell_core::prelude::{
    Animation, AnimationError, Derived, Dp, Motion, NonNegativeDp, Spring, State, Tween, Ui,
};
pub(crate) use opening::Opening;
use std::cell::RefCell;
use std::time::Duration;

const FUSING: Duration = Duration::from_millis(100);

pub(crate) fn fuse_tween() -> Tween {
    Tween::new(FUSING)
}

pub(crate) fn interpolated(closed: Dp, rest: Dp, open: f64) -> Dp {
    closed + (rest - closed) * open
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MotionPreference {
    Full,
    Reduced,
}

#[derive(Clone, Copy, Debug)]
pub struct IslandReturn {
    pub glass: Spring,
    pub content_after: Duration,
}

impl IslandReturn {
    pub fn new(ui: &Ui, distance: NonNegativeDp) -> Result<Self, AnimationError> {
        let pixel = ui.viewport().pixel();
        Ok(Self {
            glass: look::hide_spring(),
            content_after: look::hide_time(distance.get(), pixel)?,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Stage {
    Hidden,
    Growing,
    FusedOpening,
    Open,
    FusedClosing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Wait {
    Glass,
    Fade,
}

impl Stage {
    pub fn fused(self) -> bool {
        matches!(self, Self::FusedOpening | Self::FusedClosing)
    }

    fn toward(self, shows: bool, motion: MotionPreference) -> Option<Stage> {
        let reduced = motion == MotionPreference::Reduced;
        let next = match (self, shows) {
            (_, true) if reduced => Self::Open,
            (_, false) if reduced => Self::Hidden,
            (Self::Hidden, true) => Self::Growing,
            (Self::Growing, false) => Self::Hidden,
            (Self::FusedOpening | Self::Open, false) => Self::FusedClosing,
            (Self::FusedClosing, true) => Self::FusedOpening,
            (Self::Hidden | Self::FusedClosing, false)
            | (Self::Growing | Self::FusedOpening | Self::Open, true) => return None,
        };
        (next != self).then_some(next)
    }

    fn waits_for(self) -> Option<(Wait, Stage)> {
        match self {
            Self::Growing => Some((Wait::Glass, Self::FusedOpening)),
            Self::FusedOpening => Some((Wait::Fade, Self::Open)),
            Self::FusedClosing => Some((Wait::Fade, Self::Hidden)),
            Self::Hidden | Self::Open => None,
        }
    }
}

#[derive(Clone)]
struct PanelMotion {
    open: Animation<Opening>,
    rest: Animation<Dp>,
    fade: Animation<f64>,
    risen: Derived<bool>,
}

impl PanelMotion {
    fn on(clock: &Motion, pixel_share: &State<f64>) -> Result<Self, AnimationError> {
        let (open, rest, fade) = (
            clock.animation(Opening::new(0.0, Dp::ZERO))?,
            clock.animation(Dp::ZERO)?,
            clock.animation(0.0)?,
        );
        let (rising, share) = (open.clone(), pixel_share.clone());
        let risen = Derived::new(move || 1.0 - rising.get().share() <= share.get());
        Ok(Self {
            open,
            rest,
            fade,
            risen,
        })
    }
}

pub(crate) struct Stages {
    stage: State<Stage>,
    pixel_share: State<f64>,
    motion: RefCell<PanelMotion>,
}

impl Stages {
    pub fn new() -> Result<Self, AnimationError> {
        let pixel_share = State::new(0.0);
        Ok(Self {
            stage: State::new(Stage::Hidden),
            motion: RefCell::new(PanelMotion::on(&Motion::default(), &pixel_share)?),
            pixel_share,
        })
    }

    pub fn get(&self) -> Stage {
        self.stage.get()
    }

    pub fn open(&self) -> Animation<Opening> {
        self.motion.borrow().open.clone()
    }

    pub fn height(&self, closed: Dp) -> Dp {
        let motion = self.motion.borrow();
        interpolated(closed, motion.rest.get(), motion.open.get().share())
    }

    pub fn animate_rest_to(&self, height: Dp) -> Result<Animation<Dp>, AnimationError> {
        let motion = self.motion.borrow().clone();
        if self.stage.peek() == Stage::Hidden && motion.open.is_finished() {
            motion.rest.snap(height)?;
        } else {
            motion.rest.animate_to(height, look::show_spring())?;
        }
        Ok(motion.rest)
    }

    fn ended(&self, wait: Wait) -> bool {
        let motion = self.motion.borrow();
        match wait {
            Wait::Glass => motion.risen.get(),
            Wait::Fade => motion.fade.is_finished(),
        }
    }
}

impl Keyboard {
    #[must_use]
    pub fn rests_hidden(&self) -> bool {
        let stages = self.stages();
        stages.get() == Stage::Hidden && stages.open().is_finished()
    }

    pub(crate) fn advance_stage(
        &self,
        shows: bool,
        motion: MotionPreference,
        panel: Dp,
        pixel_share: f64,
    ) -> Result<(), AnimationError> {
        let stages = self.stages();
        stages.pixel_share.set_if_changed(pixel_share);
        let stage = stages.stage.peek();
        let next = stage.toward(shows, motion).or_else(|| {
            let (wait, next) = stage.waits_for()?;
            stages.ended(wait).then_some(next)
        });
        next.map_or(Ok(()), |next| self.enter_stage(next, panel))
    }

    pub(crate) fn reset_stage(&self, clock: &Motion) -> Result<(), AnimationError> {
        let stages = self.stages();
        stages.stage.set_if_changed(Stage::Hidden);
        *stages.motion.borrow_mut() = PanelMotion::on(clock, &stages.pixel_share)?;
        Ok(())
    }

    fn enter_stage(&self, stage: Stage, panel: Dp) -> Result<(), AnimationError> {
        let stages = self.stages();
        stages.stage.set(stage);
        let motion = stages.motion.borrow().clone();
        let (share, spring) = match stage {
            Stage::Hidden => (0.0, look::hide_spring()),
            _ => (1.0, look::show_spring()),
        };
        motion.open.animate_to(Opening::new(share, panel), spring)?;
        if stage.fused() {
            motion.fade.snap(0.0)?;
            motion.fade.animate_to(1.0, fuse_tween())?;
        }
        Ok(())
    }
}
