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

    pub fn semantics_dynamic(self, provider: impl Fn(&mut Semantics) + 'static) -> Self {
        let provider: SemanticsProvider = Rc::new(provider);
        self.set(NodeSetting::SemanticsProvider(provider))
    }

    pub fn on_accessibility_action(
        self,
        kind: ActionKind,
        action: impl FnMut(AccessibilityAction) -> Result<(), ActionError> + 'static,
    ) -> Self {
        let action: AccessibilityHandler = Rc::new(RefCell::new(action));
        self.set(NodeSetting::AccessibilityAction(kind, action))
    }
}
