// SPDX-License-Identifier: GPL-3.0-only
use crate::registry::Subsystem;
use lavashell_core::platform::PlatformFacts;
use lavashell_core::surface::SurfaceId;
use lavashell_core::ui::Ui;
use lavashell_core::view::View;
use lavashell_platform::{Capability, OutputId, SessionLockEvent};
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;

mod error;

pub use error::Error;

inventory::submit! { Subsystem {
    capability: Capability::SessionLock,
    used: |setup| setup.session_lock.is_some(),
} }

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Lifetime {
    #[default]
    Fresh,
    Attached,
    ReleaseRequested,
    Released,
}

#[derive(Clone, Default)]
pub struct SessionLock(Rc<Cell<Lifetime>>);

impl SessionLock {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn unlock(&self) {
        if self.0.get() == Lifetime::Attached {
            self.0.set(Lifetime::ReleaseRequested);
        }
    }

    fn attach(&self) -> Result<(), Error> {
        if self.0.get() != Lifetime::Fresh {
            return Err(Error::Reused);
        }
        self.0.set(Lifetime::Attached);
        Ok(())
    }

    fn release_requested(&self) -> bool {
        self.0.get() == Lifetime::ReleaseRequested
    }

    fn release(&self) {
        self.0.set(Lifetime::Released);
    }
}

type Compose = Rc<RefCell<dyn FnMut(&mut Ui)>>;

struct LockViews {
    platform: PlatformFacts,
    compose: Compose,
}

impl LockViews {
    fn view(&self) -> View {
        let compose = self.compose.clone();
        View::new(self.platform.clone(), move |ui| (compose.borrow_mut())(ui))
    }
}

pub struct LockConfiguration {
    handle: SessionLock,
    views: LockViews,
}

impl LockConfiguration {
    #[must_use]
    pub fn new(
        handle: SessionLock,
        platform: PlatformFacts,
        compose: impl FnMut(&mut Ui) + 'static,
    ) -> Self {
        Self {
            handle,
            views: LockViews {
                platform,
                compose: Rc::new(RefCell::new(compose)),
            },
        }
    }

    pub(crate) fn provide_platform(&mut self, platform: PlatformFacts) {
        self.views.platform = platform;
    }
}

#[derive(Clone, Copy)]
enum LockPhase {
    Requested,
    Locked,
    Unlocking,
    Unlocked,
    Finished,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum LockStep {
    Running,
    Unlock,
    Released,
}

pub(crate) struct ActiveLock {
    session_lock: SessionLock,
    phase: LockPhase,
    views: LockViews,
    surfaces: BTreeMap<OutputId, SurfaceId>,
}

impl ActiveLock {
    pub fn start(configuration: LockConfiguration, outputs: &[OutputId]) -> Result<Self, Error> {
        if outputs.is_empty() {
            return Err(Error::WithoutOutput);
        }
        configuration.handle.attach()?;
        Ok(Self {
            session_lock: configuration.handle,
            phase: LockPhase::Requested,
            views: configuration.views,
            surfaces: BTreeMap::new(),
        })
    }

    #[must_use]
    pub fn view_for(&self, output: OutputId) -> Option<View> {
        let opening = matches!(self.phase, LockPhase::Requested | LockPhase::Locked);
        (opening && !self.surfaces.contains_key(&output)).then(|| self.views.view())
    }

    pub fn opened(&mut self, output: OutputId, surface: SurfaceId) {
        let _earlier_surface_of_the_output_is_superseded = self.surfaces.insert(output, surface);
    }

    pub fn remove_output(&mut self, output: OutputId) -> Option<SurfaceId> {
        self.surfaces.remove(&output)
    }

    pub fn handle(&mut self, event: SessionLockEvent) {
        self.phase = match (event, self.phase) {
            (SessionLockEvent::Locked, LockPhase::Requested) => LockPhase::Locked,
            (SessionLockEvent::Finished, _) => LockPhase::Finished,
            (SessionLockEvent::UnlockConfirmed, LockPhase::Unlocking) => LockPhase::Unlocked,
            (_, phase) => phase,
        };
    }

    pub fn step(&mut self) -> Result<LockStep, Error> {
        match self.phase {
            LockPhase::Finished => Err(Error::Ended),
            LockPhase::Unlocked => {
                self.session_lock.release();
                Ok(LockStep::Released)
            }
            LockPhase::Locked if self.session_lock.release_requested() => {
                self.phase = LockPhase::Unlocking;
                self.surfaces.clear();
                Ok(LockStep::Unlock)
            }
            LockPhase::Requested | LockPhase::Locked | LockPhase::Unlocking => {
                Ok(LockStep::Running)
            }
        }
    }
}

#[cfg(test)]
mod tests;
