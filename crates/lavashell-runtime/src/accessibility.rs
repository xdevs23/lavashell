// SPDX-License-Identifier: GPL-3.0-only

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
use lavashell_core::id_counter::IdsExhausted;
use lavashell_core::surface::SurfaceId;
use lavashell_platform::AccessibilityRequest;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Republish {
    Always,
    WhenChanged,
}

#[derive(Default)]
enum Reading {
    #[default]
    Off,
    On(Option<u64>),
}

#[derive(Default)]
pub(crate) struct SurfaceAccessibility {
    root_name: Option<String>,
    ids: ids::Ids,
    reading: Reading,
}

impl SurfaceAccessibility {
    pub(crate) fn new(root_name: Option<String>) -> Self {
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

    pub(crate) fn publish(
        &mut self,
        surface: SurfaceId,
        tree: &AccessibilityTree,
        republish: Republish,
    ) -> Result<Option<AccessibilityRequest>, IdsExhausted> {
        let Reading::On(last) = &mut self.reading else {
            return Ok(None);
        };
        let published = *last == Some(tree.revision);
        if tree.nodes.is_empty() || republish == Republish::WhenChanged && published {
            return Ok(None);
        }
        let update = mapping::tree(tree, &mut self.ids, self.root_name.as_deref())?;
        *last = Some(tree.revision);
        Ok(Some(AccessibilityRequest::Publish {
            surface,
            tree: Box::new(update),
        }))
    }

    pub(crate) fn action(
        &self,
        request: accesskit::ActionRequest,
        tree: &AccessibilityTree,
    ) -> Option<ActionRequest> {
        if let Reading::Off = self.reading {
            return None;
        }
        actions::request(request, tree, &self.ids)
    }
}
