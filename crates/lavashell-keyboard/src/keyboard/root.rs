// SPDX-License-Identifier: GPL-3.0-only
use super::{Inner, Keyboard};
use lavashell_core::prelude::Dp;
use std::rc::{Rc, Weak};

/// Kept in the memory of the root node of a composed keyboard. Once the current root node leaves
/// its view, the keyboard takes no height.
pub(crate) struct Root {
    keyboard: Weak<Inner>,
    number: u64,
}

impl Drop for Root {
    fn drop(&mut self) {
        if let Some(inner) = self.keyboard.upgrade()
            && inner.roots.get() == self.number
        {
            inner.height.set_if_changed(Dp::ZERO);
        }
    }
}

impl Keyboard {
    /// A new root node of the keyboard, which becomes its current root node.
    pub(crate) fn root(&self) -> Root {
        let number = self.0.roots.get() + 1;
        self.0.roots.set(number);
        Root {
            keyboard: Rc::downgrade(&self.0),
            number,
        }
    }

    /// Records `height` as the height of the keyboard that the layout of its root node measured.
    pub(crate) fn record_height(&self, height: Dp) {
        self.0.height.set_if_changed(height);
    }
}
