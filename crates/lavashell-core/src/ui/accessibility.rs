// SPDX-License-Identifier: GPL-3.0-only
use super::settings::NodeSetting;
use super::{AccessibilityHandler, Modifier, SemanticsProvider};
use crate::accessibility::{AccessibilityAction, ActionError, ActionKind, Semantics};
use std::cell::RefCell;
use std::rc::Rc;

impl Modifier {
    pub fn semantics(self, semantics: Semantics) -> Self {
        self.semantics_with(move |current| current.clone_from(&semantics))
    }

    /// Changes single fields of the semantics of the node with `update`, without replacing the role or the state
    /// set by the component.
    pub fn semantics_with(self, update: impl Fn(&mut Semantics) + 'static) -> Self {
        self.set(NodeSetting::Semantics(Rc::new(update)))
    }

    pub fn accessible_name(self, name: impl Into<String>) -> Self {
        let name = name.into();
        self.semantics_with(move |semantics| semantics.name = Some(name.clone()))
    }

    pub fn accessible_description(self, description: impl Into<String>) -> Self {
        let description = description.into();
        self.semantics_with(move |semantics| semantics.description = Some(description.clone()))
    }

    /// Updates the semantics of the node with `provider` each time the view samples accessibility. The reactive
    /// state read by `provider` is tracked apart from composition and painting. `provider` must be free of side
    /// effects, because the view may call it without drawing a frame.
    pub fn semantics_dynamic(self, provider: impl Fn(&mut Semantics) + 'static) -> Self {
        let provider: SemanticsProvider = Rc::new(provider);
        self.set(NodeSetting::SemanticsProvider(provider))
    }

    /// Handles the accessibility actions of `kind` on the node with `action`. With this handler set, the node offers
    /// `kind` to assistive technology. An error returned by `action` is returned to the caller of
    /// `View::accessibility_action`.
    pub fn on_accessibility_action(
        self,
        kind: ActionKind,
        action: impl FnMut(AccessibilityAction) -> Result<(), ActionError> + 'static,
    ) -> Self {
        let action: AccessibilityHandler = Rc::new(RefCell::new(action));
        self.set(NodeSetting::AccessibilityAction(kind, action))
    }
}
