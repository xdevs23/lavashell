// SPDX-License-Identifier: GPL-3.0-only
//! Encodes the accessibility tree of each surface as an AccessKit tree, and turns the actions requested by assistive
//! technologies into Lavashell action requests.

mod actions;
mod ids;
mod mapping;
mod properties;
#[cfg(test)]
pub(crate) mod test_error;
#[cfg(test)]
mod tests;
mod text;

use lavashell_core::accessibility::{AccessibilityTree, ActionRequest};
use lavashell_core::surface::SurfaceId;
use lavashell_platform::AccessibilityRequest;

/// When a request publishes the tree of a surface.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Republish {
    /// Every time, also when the revision of the tree is the last published revision.
    Always,
    /// Once the revision of the tree differs from the last published revision.
    WhenChanged,
}

/// Whether an assistive technology reads a surface.
#[derive(Default)]
enum Reading {
    #[default]
    Off,
    /// An assistive technology reads the surface. The value is the last published revision of the tree, if any.
    On(Option<u64>),
}

/// The accessibility state of a surface: the name of its root, the AccessKit ids of its nodes, and whether an
/// assistive technology reads it.
#[derive(Default)]
pub(crate) struct SurfaceAccessibility {
    /// The label of the root node when the root is unnamed.
    root_name: String,
    ids: ids::Ids,
    reading: Reading,
}

impl SurfaceAccessibility {
    pub(crate) fn new(root_name: String) -> Self {
        Self {
            root_name,
            ..Self::default()
        }
    }

    pub(crate) fn activate(&mut self) {
        if let Reading::Off = self.reading {
            self.reading = Reading::On(None);
        }
    }

    pub(crate) fn deactivate(&mut self) {
        self.reading = Reading::Off;
    }

    /// The request that publishes `tree` for `surface`. Returns `None` while the surface is not read by an assistive
    /// technology, when `tree` is empty, and when `republish` is `WhenChanged` and the revision of `tree` is the last
    /// published revision.
    pub(crate) fn publish(
        &mut self,
        surface: SurfaceId,
        tree: &AccessibilityTree,
        republish: Republish,
    ) -> Option<AccessibilityRequest> {
        let Reading::On(last) = &mut self.reading else {
            return None;
        };
        let published = *last == Some(tree.revision);
        if tree.nodes.is_empty() || republish == Republish::WhenChanged && published {
            return None;
        }
        *last = Some(tree.revision);
        Some(AccessibilityRequest::Publish {
            surface,
            tree: Box::new(mapping::tree(tree, &mut self.ids, &self.root_name)),
        })
    }

    /// The Lavashell action that `request` of an assistive technology asks for in `tree`, while an assistive technology
    /// reads the surface.
    pub(crate) fn action(
        &self,
        request: accesskit::ActionRequest,
        tree: &AccessibilityTree,
    ) -> Option<ActionRequest> {
        let Reading::On(_) = self.reading else {
            return None;
        };
        actions::request(request, tree, &self.ids)
    }
}
