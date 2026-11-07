// SPDX-License-Identifier: GPL-3.0-only
use super::{RoutedInput, input_without_pointer};
use lavashell_core::surface::SurfaceId;
use lavashell_core::ui::TouchId;
use lavashell_core::view::{InputEvent, TouchSample};
use std::collections::BTreeMap;

/// Every touch point that is down. Each point sends input to the surface where it went down.
#[derive(Default)]
pub(super) struct TouchPoints {
    points: BTreeMap<TouchId, SurfaceId>,
}

impl TouchPoints {
    pub(super) fn down(&mut self, surface: SurfaceId, sample: TouchSample) -> RoutedInput {
        self.points.extend([(sample.id, surface)]);
        input_without_pointer(surface, InputEvent::TouchDown(sample))
    }

    pub(super) fn motion(&self, sample: TouchSample) -> Option<RoutedInput> {
        let &surface = self.points.get(&sample.id)?;
        Some(input_without_pointer(
            surface,
            InputEvent::TouchMove(sample),
        ))
    }

    pub(super) fn up(&mut self, sample: TouchSample) -> Option<RoutedInput> {
        let surface = self.points.remove(&sample.id)?;
        Some(input_without_pointer(surface, InputEvent::TouchUp(sample)))
    }

    pub(super) fn cancel(&mut self, touch: TouchId) -> Option<RoutedInput> {
        let surface = self.points.remove(&touch)?;
        Some(input_without_pointer(
            surface,
            InputEvent::TouchCancel(touch),
        ))
    }

    /// Removes the points that went down on `surface`, once the surface retires.
    pub(super) fn forget_surface(&mut self, surface: SurfaceId) {
        self.points.retain(|_, &mut down_on| down_on != surface);
    }
}

#[cfg(test)]
mod tests;
