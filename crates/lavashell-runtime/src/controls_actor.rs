// SPDX-License-Identifier: GPL-3.0-only
use crate::failures::log_failure_to_stop;
use crate::surface_slots::BuiltSurface;
use lavashell_actor::{
    Actor, Clock, Closed, Flow, Handle, JoinError, Merge, MergeEffect, Outlet, Sender, SpawnError,
    mailbox_on, spawn,
};
use lavashell_core::surface::{SurfaceId, SurfaceState};
use lavashell_error_chain::next_step::{NamesNextStep, NextStep};
use lavashell_platform::{Platform, SurfaceAttachment, SurfaceControl};
use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::sync::Arc;

pub(crate) enum ControlsInput<C: SurfaceControl> {
    Attach {
        surface: SurfaceId,
        control: C,
    },
    Control {
        surface: SurfaceId,
        message: C::Message,
    },
    Detach(SurfaceId),
    CommitState {
        surface: SurfaceId,
        built: Arc<BuiltSurface>,
    },
    Stop,
}

impl<C: SurfaceControl> Merge for ControlsInput<C> {
    type Key = ();

    fn effect(&self) -> MergeEffect<()> {
        MergeEffect::Appends
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ControlsOutput {
    Committed(SurfaceId),
    Retired(SurfaceId),
}

pub(crate) type ControlsObserver = Box<dyn FnMut(ControlsOutput) -> Result<(), Closed> + Send>;

#[derive(Debug, thiserror::Error)]
pub enum ControlsError<E> {
    #[error("the surface {0:?} is attached already")]
    AttachedTwice(SurfaceId),
    #[error("the surface {0:?} is not attached")]
    UnknownSurface(SurfaceId),
    #[error("the platform could not commit the state of a surface")]
    Control(#[source] E),
}

impl<E: NamesNextStep> NamesNextStep for ControlsError<E> {
    fn next_step(&self) -> NextStep {
        match self {
            Self::AttachedTwice(_) | Self::UnknownSurface(_) => NextStep::ReportTheFailure,
            Self::Control(error) => error.next_step(),
        }
    }
}

pub type PlatformControlsError<P> =
    ControlsError<<<P as Platform>::Control as SurfaceControl>::Error>;

pub(crate) struct SurfaceControls<C: SurfaceControl + Send + 'static> {
    inputs: Sender<ControlsInput<C>>,
    actor: Option<Handle<ControlsActor<C>>>,
}

impl<C: SurfaceControl + Send + 'static> SurfaceControls<C> {
    pub(crate) fn start(
        clock: &dyn Clock,
        observer: ControlsObserver,
    ) -> Result<Self, SpawnError<ControlsError<C::Error>>> {
        let (inputs, inbox) = mailbox_on(clock);
        let actor = spawn("surface controls", inbox, move || {
            Ok(ControlsActor {
                controls: SurfaceControlTable::default(),
                observer,
            })
        })?;
        Ok(Self {
            inputs,
            actor: Some(actor),
        })
    }

    pub(crate) fn attachments<W: 'static>(&self) -> Outlet<SurfaceAttachment<C, W>> {
        self.inputs.outlet(|attachment| match attachment {
            SurfaceAttachment::Attach(attach) => ControlsInput::Attach {
                surface: attach.surface,
                control: attach.control,
            },
            SurfaceAttachment::Control { surface, message } => {
                ControlsInput::Control { surface, message }
            }
            SurfaceAttachment::Detach(surface) => ControlsInput::Detach(surface),
        })
    }

    pub(crate) fn commit_state(
        &self,
        surface: SurfaceId,
        built: Arc<BuiltSurface>,
    ) -> Result<(), Closed> {
        self.inputs
            .push(ControlsInput::CommitState { surface, built })
    }

    pub(crate) fn stop(&mut self) -> Result<(), JoinError<ControlsError<C::Error>>> {
        let Some(actor) = self.actor.take() else {
            return Ok(());
        };
        let _controls_gone = self.inputs.push(ControlsInput::Stop);
        actor.join()
    }
}

impl<C: SurfaceControl + Send + 'static> Drop for SurfaceControls<C> {
    fn drop(&mut self) {
        log_failure_to_stop(self.stop());
    }
}

pub(crate) struct SurfaceControlTable<C> {
    controls: BTreeMap<SurfaceId, C>,
}

impl<C> Default for SurfaceControlTable<C> {
    fn default() -> Self {
        Self {
            controls: BTreeMap::new(),
        }
    }
}

impl<C: SurfaceControl> SurfaceControlTable<C> {
    pub(crate) fn attach(
        &mut self,
        surface: SurfaceId,
        control: C,
    ) -> Result<(), ControlsError<C::Error>> {
        let Entry::Vacant(entry) = self.controls.entry(surface) else {
            return Err(ControlsError::AttachedTwice(surface));
        };
        let _attached_control_is_not_needed = entry.insert(control);
        Ok(())
    }

    pub(crate) fn retire(&mut self, surface: SurfaceId) -> Option<C> {
        self.controls.remove(&surface)
    }

    pub(crate) fn attached_control(
        &mut self,
        surface: SurfaceId,
    ) -> Result<&mut C, ControlsError<C::Error>> {
        (self.controls.get_mut(&surface)).ok_or(ControlsError::UnknownSurface(surface))
    }

    pub(crate) fn commit_state(
        &mut self,
        surface: SurfaceId,
        state: &SurfaceState,
    ) -> Result<(), ControlsError<C::Error>> {
        (self.attached_control(surface)?.commit_state(state)).map_err(ControlsError::Control)
    }
}

struct ControlsActor<C> {
    controls: SurfaceControlTable<C>,
    observer: ControlsObserver,
}

impl<C: SurfaceControl + Send + 'static> Actor for ControlsActor<C> {
    type Input = ControlsInput<C>;
    type Error = ControlsError<C::Error>;

    fn handle(&mut self, input: ControlsInput<C>) -> Result<Flow, ControlsError<C::Error>> {
        let output = match input {
            ControlsInput::Attach { surface, control } => {
                self.controls.attach(surface, control)?;
                None
            }
            ControlsInput::Control { surface, message } => {
                if let Ok(control) = self.controls.attached_control(surface) {
                    control.receive(message);
                }
                None
            }
            ControlsInput::Detach(surface) => {
                drop(self.controls.retire(surface));
                Some(ControlsOutput::Retired(surface))
            }
            ControlsInput::CommitState { surface, built } => {
                self.controls.commit_state(surface, &built.state)?;
                Some(ControlsOutput::Committed(surface))
            }
            ControlsInput::Stop => return Ok(Flow::Stop),
        };
        let sent = output.into_iter().try_for_each(&mut self.observer);
        Ok(sent.map_or(Flow::Stop, |()| Flow::Continue))
    }
}

#[cfg(test)]
mod tests;
