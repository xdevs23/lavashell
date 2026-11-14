// SPDX-License-Identifier: GPL-3.0-only
use crate::Error;
use crate::steps::{Start, Steps, Work};
use lavashell::core::{keyboard::KeyStroke, surface::SurfaceId};
use lavashell::prelude::*;
use lavashell_platform::{DeviceEvent, KeyAction, KeyboardDevice};
use lavashell_test_platform::{FrameInterval, HostScript, Pace, RecordedFrame};
use std::time::Duration;

/// The keyboard of the host through which the steps press keys. A test host has this one keyboard.
const KEYBOARD: KeyboardDevice = KeyboardDevice(1);

/// A key of a keyboard of the host: its Linux evdev code, and the key that the keymap of the host gives it.
#[derive(Clone, Copy, Debug)]
pub struct KeyboardKey {
    pub code: u32,
    pub key: Key,
}

impl Start {
    /// In the first frame at least `delay` after the frame in which the step before ran, or after the frame that
    /// shows its action. The view gets a frame at that time.
    #[must_use]
    pub fn after(delay: Duration) -> Self {
        Self::after_reading(move |_| Some(delay))
    }

    /// As [`Start::after`] with the delay that `delay` reads from the view of the frame from which the delay counts.
    /// The steps fail when `delay` returns `None`.
    #[must_use]
    pub fn after_reading(delay: impl Fn(&View) -> Option<Duration> + 'static) -> Self {
        Self {
            delay: Some(Box::new(delay)),
            ..Self::NEXT
        }
    }

    /// In the first frame in which `condition` holds for the view.
    #[must_use]
    pub fn when(condition: impl Fn(&View) -> bool + 'static) -> Self {
        Self {
            condition: Some(Box::new(condition)),
            ..Self::NEXT
        }
    }

    /// In the first frame in which `condition` holds for the view, while a test platform makes a frame each `every`.
    /// A condition that a motion without deadlines changes, such as a distance during a spring, reads these frames.
    #[must_use]
    pub fn sampled(every: FrameInterval, condition: impl Fn(&View) -> bool + 'static) -> Self {
        Self {
            every: Some(every),
            ..Self::when(condition)
        }
    }
}

impl<C: 'static, L: Clone + 'static, E: From<Error> + 'static> Steps<C, L, E> {
    /// Adds `check` as the last step, which reads a view in the frame that `start` selects.
    pub fn check(
        &mut self,
        start: Start,
        check: impl FnOnce(&View, Viewport, &C) -> Result<(), E> + 'static,
    ) {
        self.add(start, Work::Check(Box::new(check)));
    }

    /// Adds a step that presses and releases each of `keys` on a keyboard of the host, in the frame that `start`
    /// selects. The keyboard focus moves to the view of that frame first. The keys reach the view through the input
    /// routing of the runtime, as keys from a real host do, and later steps wait for the frame that shows them.
    pub fn press(&mut self, start: Start, keys: impl IntoIterator<Item = KeyboardKey>) {
        self.add(start, Work::Press(keys.into_iter().collect()));
    }

    /// Plays the steps on `app` on a test platform whose host has `density`, and returns the frames as
    /// [`Steps::record`] does. The test platform plays at the pace of the steps and records the frames that the marks
    /// ask for, as [`HostScript::record_paced`] describes.
    ///
    /// # Errors
    /// Fails as [`Steps::record`] does.
    pub fn play(self, app: App, density: Density) -> Result<Vec<(L, RecordedFrame)>, E> {
        let script = HostScript::new(density).record_paced(self.pace.clone());
        self.record(app, script)
    }
}

/// Hands `pace` the keyboard focus on `surface`, then a press and a release of each of `keys`.
pub(crate) fn press(pace: &Pace, surface: SurfaceId, keys: &[KeyboardKey]) {
    pace.report(DeviceEvent::KeyboardFocus {
        keyboard: KEYBOARD,
        surface: Some(surface),
    });
    for &KeyboardKey { code, key } in keys {
        for action in [KeyAction::Pressed, KeyAction::Released] {
            let stroke = KeyStroke {
                key: Some(key),
                text: None,
            };
            pace.report(DeviceEvent::Key {
                keyboard: KEYBOARD,
                code,
                stroke,
                action,
            });
        }
    }
}
