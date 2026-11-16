// SPDX-License-Identifier: GPL-3.0-only
use super::{Inner, Keyboard};
use lavashell_core::prelude::Dp;
use std::rc::{Rc, Weak};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct RootNumber(u64);

impl RootNumber {
    fn following(self) -> Self {
        Self(self.0 + 1)
    }
}

pub(crate) struct Root {
    keyboard: Weak<Inner>,
    number: RootNumber,
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
    pub(crate) fn root(&self) -> Root {
        let number = self.0.roots.get().following();
        self.0.roots.set(number);
        Root {
            keyboard: Rc::downgrade(&self.0),
            number,
        }
    }

    pub(crate) fn record_height(&self, height: Dp) {
        self.0.height.set_if_changed(height);
    }
}
