// SPDX-License-Identifier: GPL-3.0-only
//! The stages through which the keyboard panel moves between hidden and open. Each stage between
//! the two ends when the animation awaited by that stage ends. With reduced motion, the panel goes
//! from hidden to open and back without the stages in between.

mod opening;

use crate::keyboard::Keyboard;
use crate::look;
use lavashell_core::prelude::{
    Animation, AnimationError, Derived, Dp, Motion, NonNegativeDp, Spring, State, Tween, Ui,
};
pub(crate) use opening::Opening;
use std::cell::RefCell;
use std::time::Duration;

/// How long the cross-fade lasts between the separate keys and the rows drawn as one shape each.
/// The value is a choice of the visual design.
const FUSING: Duration = Duration::from_millis(100);

pub(crate) fn fuse_tween() -> Tween {
    Tween::new(FUSING)
}

/// The animation with which the host shows an island again when the panel shrinks back into it.
/// The island is the node out of which the glass of the panel grows when the keyboard is composed
/// with `UiKeyboard::keyboard_from`.
#[derive(Clone, Copy, Debug)]
pub struct IslandReturn {
    /// The spring that moves the glass of the island. It is the spring with which the panel hides,
    /// so the island and the panel move together.
    pub glass: Spring,
    /// How long after the panel starts to hide the content of the island appears again. That is
    /// when the glass comes within one device pixel of the end of its way back.
    pub content_after: Duration,
}

impl IslandReturn {
    /// The return of glass that travels `distance` from the panel back into the island, measured
    /// at the edge of the glass that travels farthest. The view composed by `ui` gives the size of
    /// one device pixel.
    #[must_use]
    pub fn new(ui: &Ui, distance: NonNegativeDp) -> Self {
        let pixel = ui.viewport().pixel();
        Self {
            glass: look::hide_spring(),
            content_after: look::hide_time(distance.get(), pixel),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Stage {
    Hidden,
    /// The glass grows and no row shows yet.
    Growing,
    /// Each row is one shape, before the keys split apart.
    FusedOpening,
    Open,
    /// Each row is one shape, before the panel starts to hide.
    FusedClosing,
}

/// The event that ends a stage without any change to the field of the target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Wait {
    /// The glass of the panel comes within one device pixel of its full height.
    Glass,
    /// The cross-fade between the separate keys and the rows drawn as one shape ends.
    Fade,
}

impl Stage {
    pub fn fused(self) -> bool {
        matches!(self, Self::FusedOpening | Self::FusedClosing)
    }

    /// The next stage, where `shows` is whether the target has a field that takes text, or `None`
    /// when this stage continues. With `reduced` motion, the next stage is hidden or open.
    fn toward(self, shows: bool, reduced: bool) -> Option<Stage> {
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

    /// The event that ends this stage and the stage that follows it, or `None` for a stage that
    /// lasts until the target gains or loses a field that takes text.
    fn waits_for(self) -> Option<(Wait, Stage)> {
        match self {
            Self::Growing => Some((Wait::Glass, Self::FusedOpening)),
            Self::FusedOpening => Some((Wait::Fade, Self::Open)),
            Self::FusedClosing => Some((Wait::Fade, Self::Hidden)),
            Self::Hidden | Self::Open => None,
        }
    }
}

/// The animations that time the stages, on the clock of the view where the keyboard is composed.
#[derive(Clone)]
struct PanelMotion {
    /// How far the panel is open, animated with the spring of the panel.
    open: Animation<Opening>,
    /// The progress of the cross-fade between the separate keys and the rows drawn as one shape,
    /// from 0 to 1. It restarts at 0 with each cross-fade. The nodes of the rows fade with the same
    /// tween, so the stage ends when the rows finish fading.
    fade: Animation<f64>,
    /// Whether the glass of the panel is within one device pixel of its full height.
    risen: Derived<bool>,
}

impl PanelMotion {
    /// Starts the animations on `clock` with the panel hidden. `pixel` is the size of one device
    /// pixel as a share of the height of the panel.
    fn on(clock: &Motion, pixel: &State<f64>) -> Result<Self, AnimationError> {
        let (open, fade) = (
            clock.animation(Opening::new(0.0, Dp::ZERO))?,
            clock.animation(0.0)?,
        );
        let (rising, share) = (open.clone(), pixel.clone());
        let risen = Derived::new(move || 1.0 - rising.get().share() <= share.get());
        Ok(Self { open, fade, risen })
    }
}

pub(crate) struct Stages {
    stage: State<Stage>,
    pixel: State<f64>,
    motion: RefCell<PanelMotion>,
}

impl Stages {
    pub fn new() -> Result<Self, AnimationError> {
        let pixel = State::new(0.0);
        Ok(Self {
            stage: State::new(Stage::Hidden),
            motion: RefCell::new(PanelMotion::on(&Motion::default(), &pixel)?),
            pixel,
        })
    }

    /// Reading the stage during composition makes the composition run again when it changes.
    pub fn get(&self) -> Stage {
        self.stage.get()
    }

    pub fn open(&self) -> Animation<Opening> {
        self.motion.borrow().open.clone()
    }

    /// Whether the event named by `wait` has happened. Calling this during composition makes the
    /// composition run again when the event happens.
    fn ended(&self, wait: Wait) -> bool {
        let motion = self.motion.borrow();
        match wait {
            Wait::Glass => motion.risen.get(),
            Wait::Fade => motion.fade.is_finished(),
        }
    }
}

impl Keyboard {
    /// Moves the panel toward open when `shows` is true, which means that the target has a field
    /// that takes text, and toward hidden otherwise. Once the animation awaited by the current
    /// stage has ended, the panel moves on to the next stage. `panel` is the height of the panel.
    /// `pixel` is the size of one device pixel as a share of that height. Only the composition of
    /// the keyboard calls this function. Fails when the panel cannot start to move to the height
    /// `panel`.
    pub(crate) fn advance_stage(
        &self,
        shows: bool,
        reduced: bool,
        panel: Dp,
        pixel: f64,
    ) -> Result<(), AnimationError> {
        let stages = self.stages();
        stages.pixel.set_if_changed(pixel);
        let stage = stages.stage.peek();
        let next = stage.toward(shows, reduced).or_else(|| {
            let (wait, next) = stage.waits_for()?;
            stages.ended(wait).then_some(next)
        });
        next.map_or(Ok(()), |next| self.enter_stage(next, panel))
    }

    /// Sets the stage to hidden and starts new animations on `clock` with the panel closed, as for
    /// a keyboard composed into a new view.
    pub(crate) fn reset_stage(&self, clock: &Motion) -> Result<(), AnimationError> {
        let stages = self.stages();
        stages.stage.set_if_changed(Stage::Hidden);
        *stages.motion.borrow_mut() = PanelMotion::on(clock, &stages.pixel)?;
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
