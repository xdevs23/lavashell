// SPDX-License-Identifier: GPL-3.0-only
//! Namespaces of the scene of a surface written by something other than the view of the surface, at each frame of the
//! surface.

use crate::frame::FrameTime;
use crate::scene::tree::{ApplyError, CreatedNamespace, SceneTree, SceneUpdate};
use std::time::Duration;

/// Writes namespaces of the scene of one surface, which the view of the surface grafts.
pub trait NamespaceProducer {
    /// Writes into `namespaces` what changed since the last frame, after the view of the surface composed the frame of
    /// `timing` and before the surface paints it. Work requested by the producer now is due at `deadline`, when the
    /// next frame of the surface starts.
    ///
    /// # Errors
    ///
    /// Fails when the producer cannot write its namespaces.
    fn frame(
        &mut self,
        namespaces: &mut NamespaceWriter<'_>,
        timing: FrameTime,
        deadline: Duration,
    ) -> Result<(), ProducerError>;

    fn needs_frame(&self) -> bool;
}

#[derive(Debug, thiserror::Error)]
pub enum ProducerError {
    #[error("a producer could not write a namespace")]
    Apply(#[from] ApplyError),
}

/// Writes the namespaces that it creates in a scene tree. It cannot reach the root namespace of the tree, which the
/// view of the tree writes.
pub struct NamespaceWriter<'a> {
    tree: &'a mut SceneTree,
}

impl<'a> NamespaceWriter<'a> {
    #[must_use]
    pub fn new(tree: &'a mut SceneTree) -> Self {
        Self { tree }
    }

    pub fn create_namespace(&mut self) -> CreatedNamespace {
        self.tree.create_namespace()
    }

    /// Applies `update` to `namespace` as [`SceneTree::apply`] does.
    ///
    /// # Errors
    ///
    /// Fails when the tree does not have `namespace`, or with the first change that does not fit the tree.
    pub fn apply(
        &mut self,
        namespace: &CreatedNamespace,
        update: SceneUpdate,
    ) -> Result<(), ApplyError> {
        self.tree.apply(namespace.id(), update)
    }

    /// Removes `namespace` as [`SceneTree::remove_namespace`] does.
    pub fn remove_namespace(&mut self, namespace: CreatedNamespace) {
        self.tree.remove_namespace(namespace);
    }
}
