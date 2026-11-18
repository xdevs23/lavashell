// SPDX-License-Identifier: GPL-3.0-only
use crate::{model::Taskbar, script::Script};
use lavashell::core::surface::SurfaceId;
use lavashell::prelude::*;
use lavashell_example_support::report;
use std::{
    cell::{Cell, OnceCell, RefCell},
    rc::Rc,
    time::Duration,
};

pub struct Autoplay {
    script: RefCell<Script>,
    taskbar: Taskbar,
    actions: ViewActions,
    timer: OnceCell<Timer>,
    queued: Cell<bool>,
}

impl Autoplay {
    #[must_use]
    pub fn new(actions: ViewActions, taskbar: Taskbar) -> Rc<Self> {
        Rc::new(Self {
            script: RefCell::new(Script::default()),
            taskbar,
            actions,
            timer: OnceCell::new(),
            queued: Cell::new(false),
        })
    }

    pub fn observe(self: &Rc<Self>, surface: SurfaceId, view: &View) {
        if self.queued.get() || view.motion().is_active() {
            return;
        }
        self.queued.set(true);
        let timer = self.timer.get_or_init(|| {
            let autoplay = Rc::downgrade(self);
            view.motion().timer(move || {
                if let Some(autoplay) = autoplay.upgrade() {
                    autoplay.queue(surface);
                }
            })
        });
        timer.start(pause(view.motion()));
    }

    fn queue(self: &Rc<Self>, surface: SurfaceId) {
        let autoplay = self.clone();
        self.actions
            .dispatch(surface, move |view| autoplay.play(view));
    }

    fn play(&self, view: &mut View) {
        self.queued.set(false);
        if let Err(error) = self.script.borrow_mut().play(&self.taskbar, view) {
            report("taskbar script", error);
        }
    }
}

pub(crate) fn pause(motion: &Motion) -> Duration {
    (motion.skipped_rest()).map_or(Duration::ZERO, |rest| rest.saturating_sub(motion.now()))
}
