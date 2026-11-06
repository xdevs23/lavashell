// SPDX-License-Identifier: GPL-3.0-only
use super::{Record, SurfaceRecords};
use crate::AccessibilityFailure;
use crate::accessibility::Republish;
use lavashell_core::accessibility::ActionOutcome;
use lavashell_core::geometry::Viewport;
use lavashell_core::surface::SurfaceId;
use lavashell_core::view::View;
use lavashell_platform::{AccessibilityEvent, AccessibilityRequest};

/// What follows from an event of an assistive technology. `E` is the failure of the platform to show a frame.
#[must_use]
pub struct AccessibilityOutcome<E> {
    /// The request that publishes the tree of the surface.
    pub publish: Option<AccessibilityRequest>,
    /// Each failure while the runtime handled the event, in order.
    pub failures: Vec<AccessibilityFailure<E>>,
}

impl<E> Default for AccessibilityOutcome<E> {
    fn default() -> Self {
        Self {
            publish: None,
            failures: Vec::new(),
        }
    }
}

impl SurfaceRecords {
    /// Applies `event`, which an assistive technology caused. `show` shows the next frame of a view in the viewport of
    /// its surface. After a failure the runtime still runs the action and publishes the tree. An event for a surface
    /// that is not open changes nothing.
    pub fn handle_accessibility<E>(
        &mut self,
        event: AccessibilityEvent,
        mut show: impl FnMut(&mut View, Viewport) -> Result<(), E>,
    ) -> AccessibilityOutcome<E> {
        let surface = event.surface();
        let Some(record) = self.open_record_mut(surface) else {
            return AccessibilityOutcome::default();
        };
        let request = match event {
            AccessibilityEvent::Deactivate(_) => {
                record.accessibility.deactivate();
                record.view.set_accessibility_text_geometry(false);
                return AccessibilityOutcome::default();
            }
            AccessibilityEvent::Activate(_) => {
                record.accessibility.activate();
                record.view.set_accessibility_text_geometry(true);
                None
            }
            AccessibilityEvent::Action(_, request) => Some(request),
        };
        if !record.lifecycle.is_configured() {
            // The first frame after the host configures the surface publishes the complete initial tree.
            return AccessibilityOutcome::default();
        }
        let mut failures = Vec::new();
        if record.view.needs_visual_frame() || record.view.accessibility().nodes.is_empty() {
            failures.extend(record.show(&mut show));
        } else if record.view.needs_accessibility_update() {
            record.view.update_accessibility();
        }
        let action = request.and_then(|request| {
            (record.accessibility).action(request, record.view.accessibility())
        });
        let performed = match action {
            Some(action) => record.view.accessibility_action(action),
            None => Ok(ActionOutcome::Ignored),
        };
        match performed {
            Ok(ActionOutcome::Performed) => {
                // The frame is shown at once, so that the changed semantics are published at once.
                failures.extend(record.show(&mut show));
            }
            Ok(ActionOutcome::Ignored) => {}
            Err(failure) => failures.push(AccessibilityFailure::PerformAction(failure)),
        }
        AccessibilityOutcome {
            publish: record.publish(surface, Republish::Always),
            failures,
        }
    }

    /// Rebuilds the accessibility tree of every configured surface whose semantics changed without a frame, and
    /// returns the requests that publish the changes.
    pub fn update_accessibility(&mut self) -> Vec<AccessibilityRequest> {
        (self.open_records_mut())
            .filter(|(_, record)| {
                record.lifecycle.is_configured() && record.view.needs_accessibility_update()
            })
            .filter_map(|(surface, record)| {
                record.view.update_accessibility();
                record.publish(surface, Republish::WhenChanged)
            })
            .collect()
    }

    /// The request that publishes the changes of the accessibility tree of `surface` after its view showed a frame.
    pub fn publish_accessibility(&mut self, surface: SurfaceId) -> Option<AccessibilityRequest> {
        let record = self.open_record_mut(surface)?;
        record.publish(surface, Republish::WhenChanged)
    }

    /// The requests that set, for the assistive technologies, the window focus of every open surface to whether it
    /// is `focused`.
    #[must_use]
    pub fn window_focus_requests(
        &self,
        focused: impl Fn(SurfaceId) -> bool,
    ) -> Vec<AccessibilityRequest> {
        (self.open_records())
            .map(|(surface, _)| AccessibilityRequest::SetWindowFocus {
                surface,
                focused: focused(surface),
            })
            .collect()
    }
}

impl Record {
    /// Shows the next frame of the view with `show` and returns the failure of `show`, if any. The view is
    /// invalidated afterwards, so that the next render draws the surface again.
    fn show<E>(
        &mut self,
        show: &mut impl FnMut(&mut View, Viewport) -> Result<(), E>,
    ) -> Option<AccessibilityFailure<E>> {
        let shown = show(&mut self.view, Viewport::new(self.size, self.density));
        self.view.invalidate();
        shown.err().map(AccessibilityFailure::Show)
    }

    fn publish(
        &mut self,
        surface: SurfaceId,
        republish: Republish,
    ) -> Option<AccessibilityRequest> {
        (self.accessibility).publish(surface, self.view.accessibility(), republish)
    }
}
