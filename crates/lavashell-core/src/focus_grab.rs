// SPDX-License-Identifier: GPL-3.0-only
use std::rc::Rc;

#[derive(Clone)]
pub struct FocusGrab {
    active: Rc<dyn Fn() -> bool>,
    cleared: Rc<dyn Fn()>,
}

impl FocusGrab {
    pub fn new(active: impl Fn() -> bool + 'static, cleared: impl Fn() + 'static) -> Self {
        Self {
            active: Rc::new(active),
            cleared: Rc::new(cleared),
        }
    }

    #[must_use]
    pub fn is_active(&self) -> bool {
        (self.active)()
    }

    pub fn notify_cleared(&self) {
        (self.cleared)();
    }
}
