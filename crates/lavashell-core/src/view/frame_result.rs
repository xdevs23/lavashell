// SPDX-License-Identifier: GPL-3.0-only
use super::frame_error::ComponentFailure;
use crate::scene::tree::SceneUpdate;

/// A frame of a view: the update of the scene, and the components that failed since the last frame.
#[must_use]
pub struct Frame {
    pub update: SceneUpdate,
    pub failures: Failures,
}

/// The components that failed since the last frame, in the order of their failures. A component that failed twice
/// appears twice. The tree contains no node of a failed component until a value read by the component changes.
#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failures(pub(super) Vec<ComponentFailure>);

impl IntoIterator for Failures {
    type Item = ComponentFailure;
    type IntoIter = std::vec::IntoIter<ComponentFailure>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}
