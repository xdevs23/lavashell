// SPDX-License-Identifier: GPL-3.0-only
use crate::scene_item::SceneItem;
use lavashell_actor::{Choreographer, Frame};
use lavashell_core::scene::tree::{ApplyError, Change};
use lavashell_core::scene::{ChangeSink, SceneBuilder, SinkId};
use lavashell_core::surface::{SurfaceChange, SurfaceId, SurfaceState};
use std::collections::BTreeMap;
use std::convert::Infallible;

/// One choreographer per surface, placed between the view and the scene builder.
#[derive(Default)]
pub struct Choreographers {
    sinks: BTreeMap<SurfaceId, SceneSink>,
}

/// The change sink that sends the frames of a surface to a choreographer, where they merge until the scene builder
/// reads them. A frame contains the changes sent by the view and the changes of the surface state. It ends with the
/// frame of the view, or with [`SceneSink::end_state_frame`] when the view sends no frame.
#[derive(Default)]
pub struct SceneSink {
    id: SinkId,
    choreographer: Choreographer<SceneItem>,
    /// The surface state sent so far. A restart sends it again.
    state: SurfaceState,
    open: OpenFrame,
}

/// What the open frame of a scene sink contains, apart from the changes sent by the view.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum OpenFrame {
    #[default]
    WithoutState,
    WithState,
    /// The sink restarted. The open frame contains the whole surface state and waits for the whole scene, sent with
    /// the next frame of the view.
    AwaitingScene,
}

impl Choreographers {
    /// The sink for the frames of the view of `surface`.
    pub fn sink(&mut self, surface: SurfaceId) -> &mut SceneSink {
        self.sinks.entry(surface).or_default()
    }

    /// Applies the frames of `surface` that ended since the last build, merged into one frame, to `builder` and to
    /// `state`. The snapshot of `builder` changes only with a frame that changes the scene.
    ///
    /// # Errors
    ///
    /// Fails when a change does not fit the scene in `builder`. `state` still receives every change of the surface
    /// state in the frame. The sink of `surface` then has a new id. The view restarts the sink and sends the whole
    /// scene with the next frame. `builder` restarts with that frame.
    pub fn build(
        &mut self,
        surface: SurfaceId,
        builder: &mut SceneBuilder,
        state: &mut SurfaceState,
    ) -> Result<(), ApplyError> {
        self.sinks
            .get_mut(&surface)
            .map_or(Ok(()), |sink| sink.build(builder, state))
    }

    /// Drops the choreographer of `surface` once the surface retired.
    pub fn retire(&mut self, surface: SurfaceId) {
        let _retired = self.sinks.remove(&surface);
    }
}

impl SceneSink {
    /// Sends `change` of the surface state with the open frame, unless the state sent before has the value of
    /// `change`.
    pub fn set_state(&mut self, change: SurfaceChange) {
        if !self.state.contains(&change) {
            self.state.apply(change.clone());
            self.choreographer.push(SceneItem::Surface(change));
            if self.open == OpenFrame::WithoutState {
                self.open = OpenFrame::WithState;
            }
        }
    }

    /// Ends the open frame when it contains a change of the surface state and does not wait for the whole scene.
    pub fn end_state_frame(&mut self) {
        if self.open == OpenFrame::WithState {
            self.choreographer.end_frame();
            self.open = OpenFrame::WithoutState;
        }
    }

    fn build(
        &mut self,
        builder: &mut SceneBuilder,
        state: &mut SurfaceState,
    ) -> Result<(), ApplyError> {
        let Some(frame) = self.choreographer.take() else {
            return Ok(());
        };
        let built = apply(frame, builder, state);
        if built.is_err() {
            self.id = SinkId::default();
        }
        built
    }
}

/// Applies the changes of `frame` to `builder` until one of them fails, and every change of the surface state to
/// `state`. The frame ends in `builder` only when it changes the scene.
fn apply(
    frame: Frame<SceneItem>,
    builder: &mut SceneBuilder,
    state: &mut SurfaceState,
) -> Result<(), ApplyError> {
    let mut scene_changed = frame.restart;
    let mut built = if frame.restart {
        builder.restart()
    } else {
        Ok(())
    };
    for item in frame.items {
        match item {
            SceneItem::Surface(change) => state.apply(change),
            SceneItem::Change(change) => {
                scene_changed = true;
                if built.is_ok() {
                    built = builder.change(change);
                }
            }
        }
    }
    built?;
    if scene_changed {
        builder.end_frame()?;
    }
    Ok(())
}

impl ChangeSink for SceneSink {
    type Error = Infallible;

    fn id(&self) -> SinkId {
        self.id
    }

    fn change(&mut self, change: Change) -> Result<(), Infallible> {
        self.choreographer.push(SceneItem::Change(change));
        Ok(())
    }

    fn end_frame(&mut self) -> Result<(), Infallible> {
        self.choreographer.end_frame();
        self.open = OpenFrame::WithoutState;
        Ok(())
    }

    /// Drops the frames that wait for the builder, and sends the whole surface state again, because the dropped
    /// frames can contain changes of it.
    fn restart(&mut self) -> Result<(), Infallible> {
        self.choreographer.restart();
        for change in self.state.changes() {
            self.choreographer.push(SceneItem::Surface(change));
        }
        self.open = OpenFrame::AwaitingScene;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
