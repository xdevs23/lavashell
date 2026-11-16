// SPDX-License-Identifier: GPL-3.0-only
use lavashell::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Page {
    Settings,
    Display,
}

impl Page {
    fn progress(self) -> f64 {
        match self {
            Self::Display => 0.0,
            Self::Settings => 1.0,
        }
    }

    fn nearest(progress: f64) -> Self {
        if progress < 0.5 {
            Self::Display
        } else {
            Self::Settings
        }
    }
}

#[derive(Clone, Copy)]
pub enum BackMotion {
    FollowsFinger { landing: MotionSpec },
    Jumps,
}

impl BackMotion {
    fn landing(self) -> MotionSpec {
        match self {
            Self::FollowsFinger { landing } => landing,
            Self::Jumps => MotionSpec::Instant,
        }
    }
}

#[derive(Clone, Copy, Default)]
struct Grab {
    x: Dp,
    progress: f64,
}

#[derive(Clone)]
pub struct BackGesture {
    pub progress: Animation<f64>,
    page: State<Page>,
    grab: State<Grab>,
}

impl BackGesture {
    pub(super) fn new() -> Result<Self, AnimationError> {
        Ok(Self {
            progress: Animation::new(Page::Display.progress())?,
            page: State::new(Page::Display),
            grab: State::new(Grab::default()),
        })
    }

    #[must_use]
    pub fn shows_display(&self) -> bool {
        self.page.get() == Page::Display || self.progress.is_running()
    }

    pub fn go_to(&self, page: Page, motion: BackMotion) -> Result<(), AnimationError> {
        self.land(page, motion.landing())
    }

    pub fn drag(&self, drag: DragInput, motion: BackMotion) -> Result<(), AnimationError> {
        match drag.phase {
            DragPhase::Started => {
                let progress = self.progress.get();
                self.grab.set(Grab {
                    x: drag.position.x,
                    progress,
                });
                Ok(())
            }
            DragPhase::Moved => match motion {
                BackMotion::FollowsFinger { .. } => self.progress.snap(self.dragged(drag)),
                BackMotion::Jumps => Ok(()),
            },
            DragPhase::Ended => self.go_to(Page::nearest(self.dragged(drag)), motion),
            DragPhase::Cancelled => self.go_to(self.page.get(), motion),
        }
    }

    fn dragged(&self, drag: DragInput) -> f64 {
        let grab = self.grab.get();
        let share = (drag.position.x - grab.x) / drag.size.width;
        (grab.progress + share).clamp(0.0, 1.0)
    }

    fn land(&self, page: Page, landing: MotionSpec) -> Result<(), AnimationError> {
        self.page.set(page);
        self.progress.animate_to(page.progress(), landing)
    }

    pub(super) fn settle(&self) -> Result<(), AnimationError> {
        self.progress.snap(self.progress.target())
    }
}
