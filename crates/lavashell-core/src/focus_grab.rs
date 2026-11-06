// SPDX-License-Identifier: GPL-3.0-only
//! The focus grab of a panel, which limits input focus to the panel.
use std::rc::Rc;

/// The focus grab of a panel, such as a popover. The panel grabs focus while `active` returns true. The host clears
/// the grab when the user interacts outside the panel or when another focus grab starts, and then `cleared` runs. A
/// new grab starts only after `active` returned false once. A host without focus grabs applies the focus rules of
/// that host to the panel.
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

    /// Whether the panel asks to grab focus at this moment.
    #[must_use]
    pub fn is_active(&self) -> bool {
        (self.active)()
    }

    /// Runs `cleared`, after the host cleared the grab.
    pub fn notify_cleared(&self) {
        (self.cleared)();
    }
}
