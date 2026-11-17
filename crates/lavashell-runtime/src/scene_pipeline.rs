// SPDX-License-Identifier: GPL-3.0-only
use crate::builder_actor::BuilderActor;
use crate::choreographer_actor::{ChoreographerActor, ChoreographerInput};
use crate::failures::log_failure_to_stop;
use crate::scene_item::SceneItem;
use crate::scheduler::FrameSequence;
use crate::surface_slots::{BuiltSurface, SurfaceSlots};
use lavashell_actor::{Clock, Closed, Handle, Sender, mailbox_on, spawn};
use lavashell_core::scene::tree::{ApplyError, Change};
use lavashell_core::scene::{ChangeSink, SinkId};
use lavashell_core::surface::{SurfaceChange, SurfaceId, SurfaceState};
use lavashell_core::view::View;
use lavashell_error_chain::message_chain;
use std::collections::BTreeMap;
use std::mem;
use std::sync::Arc;

mod error;

pub use error::Error;

#[derive(Clone, Copy, Debug)]
pub enum BuilderReport {
    Built {
        surface: SurfaceId,
        sequence: Option<FrameSequence>,
    },
    StateChanged {
        surface: SurfaceId,
    },
    Restart {
        surface: SurfaceId,
        error: ApplyError,
    },
}

pub type BuilderObserver = Box<dyn FnMut(BuilderReport) -> Result<(), Closed> + Send>;

pub struct ScenePipeline {
    choreographer: Sender<ChoreographerInput>,
    sinks: BTreeMap<SurfaceId, SinkState>,
    slots: Arc<SurfaceSlots>,
    run: Run,
}

enum Run {
    Running(Handle<ChoreographerActor>, Handle<BuilderActor>),
    Ended(Result<(), Arc<Error>>),
}

pub struct SceneSink<'a> {
    surface: SurfaceId,
    end: Option<FrameSequence>,
    state: &'a mut SinkState,
    choreographer: &'a Sender<ChoreographerInput>,
}

#[derive(Default)]
struct SinkState {
    id: SinkId,
    any_frame_ended: bool,
    surface: SurfaceState,
    open: OpenFrame,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum OpenFrame {
    #[default]
    WithoutState,
    WithState,
    AwaitingScene,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameStatus {
    Ended,
    Open,
}

impl ScenePipeline {
    pub fn start(observer: BuilderObserver, clock: &dyn Clock) -> Result<Self, Error> {
        let (choreographer, choreographer_inbox) = mailbox_on(clock);
        let (builder, builder_inbox) = mailbox_on(clock);
        let intake = choreographer_inbox.intake();
        let choreographer_actor = spawn("choreographer", choreographer_inbox, move || {
            Ok(ChoreographerActor::new(builder, intake))
        })
        .map_err(Error::StartChoreographer)?;
        let slots = Arc::new(SurfaceSlots::default());
        let (built, requesting) = (Arc::clone(&slots), choreographer.clone());
        let spawned = spawn("scene builder", builder_inbox, move || {
            Ok(BuilderActor::new(built, requesting, observer))
        });
        let builder_actor = match spawned {
            Ok(builder_actor) => builder_actor,
            Err(error) => {
                drop(choreographer);
                let _choreographer_end = choreographer_actor.join();
                return Err(Error::StartBuilder(error));
            }
        };
        Ok(Self {
            choreographer,
            sinks: BTreeMap::new(),
            slots,
            run: Run::Running(choreographer_actor, builder_actor),
        })
    }

    pub fn sink(&mut self, surface: SurfaceId, end: Option<FrameSequence>) -> SceneSink<'_> {
        SceneSink {
            surface,
            end,
            state: self.sinks.entry(surface).or_default(),
            choreographer: &self.choreographer,
        }
    }

    pub fn restart_view(&mut self, surface: SurfaceId, view: &mut View, error: &ApplyError) {
        log::warn!(
            "a frame did not fit the built scene. The next frame sends the whole scene: {}",
            message_chain(error)
        );
        if let Some(sink) = self.sinks.get_mut(&surface) {
            sink.id = SinkId::default();
        }
        view.invalidate();
    }

    #[must_use]
    pub fn built(&self, surface: SurfaceId) -> Arc<BuiltSurface> {
        self.slots.read(surface)
    }

    pub fn retire(&mut self, surface: SurfaceId) -> Result<(), Closed> {
        let _retired_sink = self.sinks.remove(&surface);
        self.choreographer.push(ChoreographerInput::Retire(surface))
    }

    pub fn stop(&mut self) -> Result<(), Arc<Error>> {
        let ended = match mem::replace(&mut self.run, Run::Ended(Ok(()))) {
            Run::Running(choreographer, builder) => {
                let _choreographer_gone = self.choreographer.push(ChoreographerInput::Stop);
                let choreographer = choreographer.join().map_err(Error::RunChoreographer);
                let builder = builder.join().map_err(Error::RunBuilder);
                builder.and(choreographer).map_err(Arc::new)
            }
            Run::Ended(ended) => ended,
        };
        self.run = Run::Ended(ended.clone());
        ended
    }
}

impl Drop for ScenePipeline {
    fn drop(&mut self) {
        if matches!(self.run, Run::Running(..)) {
            log_failure_to_stop(self.stop());
        }
    }
}

impl SceneSink<'_> {
    #[must_use]
    pub fn any_frame_ended(&self) -> bool {
        self.state.any_frame_ended
    }

    pub fn set_state(&mut self, change: SurfaceChange) -> Result<(), Closed> {
        if self.state.surface.contains(&change) {
            return Ok(());
        }
        self.push(SceneItem::Surface(change.clone()))?;
        self.state.surface.apply(change);
        if self.state.open == OpenFrame::WithoutState {
            self.state.open = OpenFrame::WithState;
        }
        Ok(())
    }

    pub fn end_state_frame(&mut self) -> Result<FrameStatus, Closed> {
        let status = match (self.state.open, self.end) {
            (OpenFrame::WithState, _) | (OpenFrame::WithoutState, Some(_)) => FrameStatus::Ended,
            (OpenFrame::WithoutState, None) | (OpenFrame::AwaitingScene, _) => FrameStatus::Open,
        };
        if status == FrameStatus::Ended {
            (self.choreographer).push(ChoreographerInput::End(self.surface, self.end))?;
            self.state.open = OpenFrame::WithoutState;
        }
        Ok(status)
    }

    fn push(&self, item: SceneItem) -> Result<(), Closed> {
        (self.choreographer).push(ChoreographerInput::Change(self.surface, Box::new(item)))
    }
}

impl ChangeSink for SceneSink<'_> {
    type Error = Closed;

    fn id(&self) -> SinkId {
        self.state.id
    }

    fn change(&mut self, change: Change) -> Result<(), Closed> {
        self.push(SceneItem::Change(change))
    }

    fn end_frame(&mut self) -> Result<(), Closed> {
        (self.choreographer).push(ChoreographerInput::End(self.surface, self.end))?;
        self.state.any_frame_ended = true;
        self.state.open = OpenFrame::WithoutState;
        Ok(())
    }

    fn restart(&mut self) -> Result<(), Closed> {
        (self.choreographer).push(ChoreographerInput::Restart(self.surface))?;
        for change in self.state.surface.changes() {
            self.push(SceneItem::Surface(change))?;
        }
        self.state.open = OpenFrame::AwaitingScene;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
