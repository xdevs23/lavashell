// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::geometry::Viewport;
use lavashell_core::platform::PlatformFacts;
use lavashell_core::preferences::{
    CONTRAST, PreferenceValues, REDUCED_MOTION, REDUCED_TRANSPARENCY, TEXT_SCALING,
};
use lavashell_core::view::View;
use lavashell_test_fixtures::{FrameFailure, NoText, RecordedScene, WithoutFailures};
use std::{cell::RefCell, rc::Rc};

/// The preference values read by a view at its root in its latest composition.
pub(crate) struct PreferenceReader {
    values: Rc<RefCell<PreferenceValues>>,
}

impl PreferenceReader {
    /// A view that records the preference values read at its root when it composes, and the reader of those values.
    pub fn view(platform: PlatformFacts) -> (View, Self) {
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
        (view, Self { values })
    }

    /// Frames `view` and returns the values read by its latest composition.
    pub fn values(&self, view: &mut View) -> Result<PreferenceValues, FrameFailure> {
        let mut scene = RecordedScene::default();
        (view.frame_into(Viewport::default(), &mut NoText, &mut scene)).without_failures()?;
        Ok(self.values.borrow().clone())
    }
}
