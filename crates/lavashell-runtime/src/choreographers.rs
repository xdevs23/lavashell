// SPDX-License-Identifier: GPL-3.0-only
use crate::scene_item::SceneItem;
use lavashell_actor::{Choreographer, Frame};
use lavashell_core::scene::tree::{ApplyError, Change};
use lavashell_core::scene::{ChangeSink, SceneBuilder, SinkId};
use lavashell_core::surface::SurfaceId;
use std::collections::BTreeMap;
use std::convert::Infallible;

/// One choreographer per surface, placed between the view and the scene builder.
#[derive(Default)]
pub struct Choreographers {
    sinks: BTreeMap<SurfaceId, SceneSink>,
}

/// The change sink that sends the frames of a view to a choreographer, where they merge until the scene builder
/// takes them.
#[derive(Default)]
pub struct SceneSink {
    id: SinkId,
    choreographer: Choreographer<SceneItem>,
}

impl Choreographers {
    /// The sink for the frames of the view of `surface`.
    pub fn sink(&mut self, surface: SurfaceId) -> &mut SceneSink {
        self.sinks.entry(surface).or_default()
    }

    /// Applies the frames of `surface` that ended since the last build to `builder`, merged into one frame.
    ///
    /// # Errors
    ///
    /// Fails when a change does not fit the scene in `builder`. The sink of `surface` then has a new id. The view
    /// restarts the sink and sends the whole scene with the next frame. `builder` restarts with that frame.
    pub fn build(
        &mut self,
        surface: SurfaceId,
        builder: &mut SceneBuilder,
    ) -> Result<(), ApplyError> {
        self.sinks
            .get_mut(&surface)
            .map_or(Ok(()), |sink| sink.build(builder))
    }

    /// Drops the choreographer of `surface` once the surface retired.
    pub fn retire(&mut self, surface: SurfaceId) {
        let _retired = self.sinks.remove(&surface);
    }
}

impl SceneSink {
    fn build(&mut self, builder: &mut SceneBuilder) -> Result<(), ApplyError> {
        let Some(frame) = self.choreographer.take() else {
            return Ok(());
        };
        let built = apply(frame, builder);
        if built.is_err() {
            self.id = SinkId::default();
        }
        built
    }
}

fn apply(frame: Frame<SceneItem>, builder: &mut SceneBuilder) -> Result<(), ApplyError> {
    if frame.restart {
        builder.restart()?;
    }
    for SceneItem(change) in frame.items {
        builder.change(change)?;
    }
    builder.end_frame()
}

impl ChangeSink for SceneSink {
    type Error = Infallible;

    fn id(&self) -> SinkId {
        self.id
    }

    fn change(&mut self, change: Change) -> Result<(), Infallible> {
        self.choreographer.push(SceneItem(change));
        Ok(())
    }

    fn end_frame(&mut self) -> Result<(), Infallible> {
        self.choreographer.end_frame();
        Ok(())
    }

    fn restart(&mut self) -> Result<(), Infallible> {
        self.choreographer.restart();
        Ok(())
    }
}

#[cfg(test)]
mod tests;
