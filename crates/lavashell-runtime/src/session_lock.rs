// SPDX-License-Identifier: GPL-3.0-only
use crate::Error;
use lavashell_core::platform::PlatformFacts;
use lavashell_core::surface::SurfaceId;
use lavashell_core::ui::Ui;
use lavashell_core::view::View;
use lavashell_platform::{LockEvent, OutputId};
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Lifetime {
    #[default]
    Fresh,
    Attached,
    ReleaseRequested,
    Released,
}

/// The handle used by an app to unlock the session locked by the app. A handle belongs to one lock request, which
/// attaches it. Dropping the handle leaves the session locked. `unlock` does nothing before the lock request.
/// Attaching a handle that was already attached fails.
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
            return Err(Error::SessionLockReused);
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

/// The views of the lock surfaces. Every lock surface shows a view built by the same compose function, with the
/// platform facts supplied by the app.
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

/// The session lock set by an app before the run: the handle that unlocks it and the content of its surfaces.
pub struct LockConfiguration {
    handle: SessionLock,
    views: LockViews,
}

impl LockConfiguration {
    /// Locks the session until an unlock through `handle`. Each lock surface shows a view composed by `compose`, with
    /// `platform` as the platform facts supplied by the app.
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
}

/// The phase of the session lock.
#[derive(Clone, Copy)]
enum LockPhase {
    /// The host has not locked the session yet.
    Requested,
    Locked,
    /// The platform sent the unlock, and the host has not processed it yet.
    Unlocking,
    Unlocked,
    /// The host ended the lock, or did not lock the session at all.
    Finished,
}

/// What the platform does after the lock step of a turn.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum LockStep {
    Running,
    /// The platform unlocks the session and closes every surface.
    Unlock,
    /// The host processed the unlock, so the run ends.
    Released,
}

/// The session lock from its start until the host unlocks it, with one lock surface per output.
pub(crate) struct ActiveLock {
    session_lock: SessionLock,
    phase: LockPhase,
    views: LockViews,
    surfaces: BTreeMap<OutputId, SurfaceId>,
}

impl ActiveLock {
    /// Starts the lock set by `configuration` on the host with `outputs`. The platform then locks the session and
    /// opens a lock surface on each output with the view from `view_for`.
    ///
    /// # Errors
    ///
    /// Fails when `outputs` is empty, because a lock without a surface would leave the session locked without
    /// content, and when the handle of `configuration` was attached before.
    pub fn start(configuration: LockConfiguration, outputs: &[OutputId]) -> Result<Self, Error> {
        if outputs.is_empty() {
            return Err(Error::SessionLockWithoutOutput);
        }
        configuration.handle.attach()?;
        Ok(Self {
            session_lock: configuration.handle,
            phase: LockPhase::Requested,
            views: configuration.views,
            surfaces: BTreeMap::new(),
        })
    }

    /// The view for a new lock surface on `output`. `None` when `output` has a lock surface, and once the unlock was
    /// sent or the host ended the lock.
    #[must_use]
    pub fn view_for(&self, output: OutputId) -> Option<View> {
        let opening = matches!(self.phase, LockPhase::Requested | LockPhase::Locked);
        (opening && !self.surfaces.contains_key(&output)).then(|| self.views.view())
    }

    /// Records `surface` as the lock surface on `output`, opened by the platform with the view from `view_for`.
    pub fn opened(&mut self, output: OutputId, surface: SurfaceId) {
        let _replaced = self.surfaces.insert(output, surface);
    }

    /// Removes the lock surface on `output` from the lock and returns it, so the surface closes once the host removed
    /// the output.
    pub fn remove_output(&mut self, output: OutputId) -> Option<SurfaceId> {
        self.surfaces.remove(&output)
    }

    /// Applies `event` reported by the host.
    pub fn handle(&mut self, event: LockEvent) {
        self.phase = match (event, self.phase) {
            (LockEvent::Locked, LockPhase::Requested) => LockPhase::Locked,
            (LockEvent::Finished, _) => LockPhase::Finished,
            (LockEvent::UnlockConfirmed, LockPhase::Unlocking) => LockPhase::Unlocked,
            (_, phase) => phase,
        };
    }

    /// Returns what the platform does in this turn. Once the session is locked, an unlock through the handle returns
    /// `Unlock`, which the platform sends in the same turn, so the lock counts as unlocking from then on. Once the host
    /// processed the unlock, the handle is released.
    ///
    /// # Errors
    ///
    /// Fails when the host ended the lock or did not lock the session.
    pub fn step(&mut self) -> Result<LockStep, Error> {
        match self.phase {
            LockPhase::Finished => Err(Error::SessionLockEnded),
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
