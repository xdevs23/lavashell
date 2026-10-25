// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::preferences::{
    CONTRAST, PreferenceValues, REDUCED_MOTION, REDUCED_TRANSPARENCY, TEXT_SCALING,
};
use lavashell_core::prelude::*;
use std::{cell::RefCell, rc::Rc};

struct NoText;

impl TextMeasurer for NoText {
    fn measure(&mut self, _: &str, _: &TextStyle<Dp>, _: Dp) -> Size {
        Size::ZERO
    }
}

/// A view that records the preference values read at the root of the view when it composes.
pub(crate) struct PreferenceReader {
    pub view: View,
    values: Rc<RefCell<PreferenceValues>>,
}

impl PreferenceReader {
    pub fn new(platform: PlatformFacts) -> Self {
        let values = Rc::new(RefCell::new(PreferenceValues::default()));
        let record = values.clone();
        let view = View::new(platform, move |ui| {
            *record.borrow_mut() = PreferenceValues {
                text_scaling: ui.local(&TEXT_SCALING),
                reduced_motion: ui.local(&REDUCED_MOTION),
                reduced_transparency: ui.local(&REDUCED_TRANSPARENCY),
                contrast: ui.local(&CONTRAST),
            };
        });
        Self { view, values }
    }

    /// Frames the view and returns the values read by its latest composition.
    pub fn values(&mut self) -> PreferenceValues {
        if let Err(error) = self.view.frame(Viewport::default(), &mut NoText) {
            log::warn!("the view that reads the preferences could not frame: {error}");
        }
        self.values.borrow().clone()
    }
}
