// SPDX-License-Identifier: GPL-3.0-only
use crate::accessibility::Bridge;
use crate::translator::Translator;
use lavashell_core::surface::SurfaceId;
use lavashell_platform::AccessibilityRequest;

impl Translator {
    /// Executes `request` of the runtime on the accessibility bridge of the surface. A destroyed surface ignores the
    /// request.
    pub(crate) fn execute_accessibility(&mut self, request: AccessibilityRequest) {
        match request {
            AccessibilityRequest::Publish { surface, tree } => {
                if let Some(bridge) = self.bridge_mut(surface) {
                    bridge.publish(*tree);
                }
            }
            AccessibilityRequest::SetWindowFocus { surface, focused } => {
                if let Some(bridge) = self.bridge_mut(surface) {
                    bridge.window_focus(focused);
                }
            }
        }
    }

    fn bridge_mut(&mut self, surface: SurfaceId) -> Option<&mut Bridge> {
        self.surfaces.live_mut(surface)?.accessibility.as_mut()
    }
}
