// SPDX-License-Identifier: GPL-3.0-only
//! Namespaces of the scene of a surface written by something other than its view, and the producers that write
//! them at each frame of the surface.

use super::{FrameTime, SceneTree, Surface, View};
use crate::Error;
use lavashell_core::producer::{NamespaceProducer, NamespaceWriter};
use lavashell_core::scene::tree::ApplyError;
use lavashell_core::view::FrameError;

#[cfg(test)]
mod tests;

/// The scene of a new surface with the producers that write its namespaces.
#[derive(Default)]
pub(super) struct SurfaceScene {
    pub tree: SceneTree,
    pub producers: Vec<Box<dyn NamespaceProducer>>,
}

impl SurfaceScene {
    /// Runs `start` with a new scene before the first frame of a surface, and returns the view made by `start` with
    /// that scene.
    pub(super) fn start(start: impl FnOnce(&mut SurfaceStart<'_>) -> View) -> (View, Self) {
        let mut scene = Self::default();
        let view = start(&mut SurfaceStart { scene: &mut scene });
        (view, scene)
    }
}

/// A surface that has not shown its first frame yet, with its scene.
pub struct SurfaceStart<'a> {
    scene: &'a mut SurfaceScene,
}

impl SurfaceStart<'_> {
    /// The namespaces of the scene of the surface, which the composition of the surface can graft.
    pub fn namespaces(&mut self) -> NamespaceWriter<'_> {
        NamespaceWriter::new(&mut self.scene.tree)
    }

    /// Lets `producer` write into the scene at every frame of the surface, for as long as the surface exists.
    pub fn produce(&mut self, producer: impl NamespaceProducer + 'static) {
        self.scene.producers.push(Box::new(producer));
    }
}

impl Surface {
    pub(super) fn producers_need_frame(&self) -> bool {
        self.producers.iter().any(|producer| producer.needs_frame())
    }

    /// Lets the producers of the surface write their namespaces for the frame of `timing`, after the view composed it,
    /// in the order in which they were added.
    pub(super) fn produce(&mut self, timing: FrameTime) -> Result<(), Error> {
        let deadline = self.clock.next_frame_start();
        let mut namespaces = NamespaceWriter::new(&mut self.tree);
        for producer in &mut self.producers {
            (producer.frame(&mut namespaces, timing, deadline)).map_err(Error::Produce)?;
        }
        Ok(())
    }
}

/// Runs `frame` to apply the next frame of a view to `tree`. When `tree` fails to apply the update after its root
/// namespace showed a scene, the view has emptied the root namespace and sends the whole scene with its next frame,
/// so the failure is returned as `Ok(Some(error))`. On a tree whose root namespace showed nothing, that failure is an
/// error like every other failure, whatever the producers wrote into the other namespaces.
pub(super) fn apply_frame(
    tree: &mut SceneTree,
    frame: impl FnOnce(&mut SceneTree) -> Result<(), FrameError>,
) -> Result<Option<ApplyError>, FrameError> {
    let showed_nothing = tree.is_root_empty();
    match frame(tree) {
        Ok(()) => Ok(None),
        Err(FrameError::Apply(error)) if !showed_nothing => Ok(Some(error)),
        Err(error) => Err(error),
    }
}
