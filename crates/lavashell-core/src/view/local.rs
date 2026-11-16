// SPDX-License-Identifier: GPL-3.0-only
use super::View;
use crate::local::Local;
use crate::preferences::{
    CONTRAST, PreferenceValues, REDUCED_MOTION, REDUCED_TRANSPARENCY, TEXT_SCALING,
};
use crate::window::{WINDOW, WindowHandle, WindowState};

impl View {
    pub fn provide<T: Clone + PartialEq + 'static>(&self, key: &'static Local<T>, value: T) {
        self.root_values.provide(key, value);
    }

    pub fn provide_preferences(&self, values: &PreferenceValues) {
        self.provide(&TEXT_SCALING, values.text_scaling.clone());
        self.provide(&REDUCED_MOTION, values.reduced_motion);
        self.provide(&REDUCED_TRANSPARENCY, values.reduced_transparency);
        self.provide(&CONTRAST, values.contrast);
    }

    pub fn provide_window(&self, state: WindowState) {
        self.provide(&WINDOW, Some(WindowHandle::new(state)));
    }
}
