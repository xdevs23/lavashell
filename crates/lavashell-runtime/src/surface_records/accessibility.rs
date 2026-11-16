// SPDX-License-Identifier: GPL-3.0-only
use super::{Error, Record, SurfaceRecords};
use crate::accessibility::Republish;
use lavashell_core::accessibility::{ActionError, ActionOutcome};
use lavashell_core::geometry::Viewport;
use lavashell_core::surface::SurfaceId;
use lavashell_core::view::View;
use lavashell_platform::{AccessibilityEvent, AccessibilityRequest};

#[derive(Debug, thiserror::Error)]
pub enum AccessibilityFailure<E> {
    #[error("a surface could not show the frame that an assistive technology reads")]
    Show(#[source] E),
    #[error("the app could not perform an action requested by an assistive technology")]
    PerformAction(#[source] ActionError),
    #[error("the accessibility tree of a surface could not be published")]
    Publish(#[source] Error),
}

#[must_use]
pub struct AccessibilityOutcome<E> {
    pub publish: Option<AccessibilityRequest>,
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
        match action.map(|action| record.view.accessibility_action(action)) {
            Some(Ok(ActionOutcome::Performed)) => {
                failures.extend(record.show(&mut show));
            }
            Some(Ok(ActionOutcome::Ignored(_))) | None => {}
            Some(Err(failure)) => failures.push(AccessibilityFailure::PerformAction(failure)),
        }
        let publish = record
            .publish(surface, Republish::Always)
            .unwrap_or_else(|failure| {
                failures.push(AccessibilityFailure::Publish(failure));
                None
            });
        AccessibilityOutcome { publish, failures }
    }

    pub fn update_accessibility(&mut self) -> Result<Vec<AccessibilityRequest>, Error> {
        (self.open_records_mut())
            .filter(|(_, record)| {
                record.lifecycle.is_configured() && record.view.needs_accessibility_update()
            })
            .filter_map(|(surface, record)| {
                record.view.update_accessibility();
                record.publish(surface, Republish::WhenChanged).transpose()
            })
            .collect()
    }

    pub fn publish_accessibility(
        &mut self,
        surface: SurfaceId,
    ) -> Result<Option<AccessibilityRequest>, Error> {
        match self.open_record_mut(surface) {
            Some(record) => record.publish(surface, Republish::WhenChanged),
            None => Ok(None),
        }
    }

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
    ) -> Result<Option<AccessibilityRequest>, Error> {
        (self.accessibility)
            .publish(surface, self.view.accessibility(), republish)
            .map_err(Error::NumberAccessibleNode)
    }
}
