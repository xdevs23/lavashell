// SPDX-License-Identifier: GPL-3.0-only
use super::View;
use crate::local::Local;
use crate::preferences::{
    CONTRAST, PreferenceValues, REDUCED_MOTION, REDUCED_TRANSPARENCY, TEXT_SCALING,
};

impl View {
    /// Sets the root value of `key` in this view. A read of `key` returns the root value where no enclosing node
    /// provides another value. For a key whose fallback comes from the platform facts, `value` replaces that fallback
    /// from then on. Only the parts of the view that read a changed value compose again.
    pub fn provide<T: Clone + PartialEq + 'static>(&self, key: &'static Local<T>, value: T) {
        self.root_values.provide(key, value);
    }

    /// Sets each of `values` as the root value of the matching preference key. A host calls this whenever the
    /// preferences of the platform change.
    pub fn provide_preferences(&self, values: &PreferenceValues) {
        self.provide(&TEXT_SCALING, values.text_scaling.clone());
        self.provide(&REDUCED_MOTION, values.reduced_motion);
        self.provide(&REDUCED_TRANSPARENCY, values.reduced_transparency);
        self.provide(&CONTRAST, values.contrast);
    }
}
