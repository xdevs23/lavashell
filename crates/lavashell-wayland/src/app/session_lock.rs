// SPDX-License-Identifier: GPL-3.0-only
use super::creation::SurfaceSpec;
use super::namespaces::SurfaceScene;
use super::{
    Connection, Dispatch, GlobalList, OutputRequest, Platform, QueueHandle, Request, Role,
    SurfaceStart, View, delegate_noop, sctk, wl_output, wl_surface,
};
use crate::Error;
use sctk::reexports::client::protocol::wl_callback;
use sctk::reexports::protocols::ext::session_lock::v1::client::{
    ext_session_lock_manager_v1::ExtSessionLockManagerV1,
    ext_session_lock_surface_v1::{self, ExtSessionLockSurfaceV1},
    ext_session_lock_v1::{self, ExtSessionLockV1},
};
use std::{cell::Cell, num::NonZeroU32, rc::Rc};

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Lifetime {
    #[default]
    Fresh,
    Attached,
    ReleaseRequested,
    Released,
}

/// The handle with which an app unlocks the session that it locked. A handle belongs to one lock request, which
/// attaches it. Dropping the handle leaves the session locked. `unlock` has no effect before the lock request.
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
}

/// Starts the view of each lock surface with the scene of that surface.
pub(super) type LockStart = Box<dyn FnMut(&mut SurfaceStart<'_>) -> View>;

pub(super) struct Configuration {
    handle: SessionLock,
    start: LockStart,
}

impl Configuration {
    pub fn new(handle: SessionLock, start: LockStart) -> Self {
        Self { handle, start }
    }
}

pub(super) type LockSurface = ExtSessionLockSurfaceV1;

/// Where the session lock is in its life.
enum LockPhase {
    /// The compositor has not locked the session yet.
    Requested,
    Locked,
    /// The unlock request is sent. The compositor has not yet answered the sync request sent after it.
    Unlocking(wl_callback::WlCallback),
    /// The compositor processed the unlock.
    Unlocked,
    /// The compositor sent `finished`, because it did not lock the session or because it ended the lock.
    Finished,
}

pub(super) struct Runtime {
    proxy: ExtSessionLockV1,
    handle: SessionLock,
    phase: LockPhase,
    start: LockStart,
    outputs: Vec<(wl_output::WlOutput, usize)>,
    failure: Option<Error>,
}

/// Whether the event loop continues for the session lock, or ends because the compositor unlocked the session.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum LockState {
    Running,
    Released,
}

impl Runtime {
    pub fn new(
        globals: &GlobalList,
        qh: &QueueHandle<Platform>,
        configuration: Configuration,
        outputs: impl Iterator<Item = wl_output::WlOutput>,
        requests: &mut Vec<Request>,
    ) -> Result<Self, Error> {
        let manager: ExtSessionLockManagerV1 = globals
            .bind(qh, 1..=1, ())
            .map_err(Error::BindSessionLock)?;
        let outputs: Vec<_> = outputs.collect();
        if outputs.is_empty() {
            return Err(Error::SessionLockWithoutOutput);
        }
        configuration.handle.attach()?;
        let proxy = manager.lock(qh, ());
        manager.destroy();
        requests.extend(outputs.into_iter().map(Request::Lock));
        Ok(Self {
            proxy,
            handle: configuration.handle,
            phase: LockPhase::Requested,
            start: configuration.start,
            outputs: Vec::new(),
            failure: None,
        })
    }
}

impl SurfaceSpec {
    pub(super) fn lock(view: View, scene: SurfaceScene) -> Self {
        Self {
            view,
            scene,
            size: (1, 1),
            transparent: false,
            output: OutputRequest::default(),
            hdr_metadata: None,
            background_blur: false,
            shape: None,
            drag_region: None,
            accessible_name: String::new(),
        }
    }
}

impl Platform {
    pub(super) fn lock_role(
        &mut self,
        wl: &wl_surface::WlSurface,
        output: &wl_output::WlOutput,
        id: usize,
    ) -> Result<Role, Error> {
        let lock = self
            .session_lock
            .as_mut()
            .ok_or(Error::LockSurfaceWithoutLock)?;
        lock.outputs.push((output.clone(), id));
        Ok(Role::Lock(
            lock.proxy.get_lock_surface(wl, output, &self.qh, id),
        ))
    }

    pub(super) fn lock_output_added(&mut self, output: &wl_output::WlOutput) {
        if let Err(error) = self.add_lock_output(output)
            && let Some(lock) = &mut self.session_lock
        {
            lock.failure = Some(error);
        }
    }

    pub(super) fn add_lock_output(&mut self, output: &wl_output::WlOutput) -> Result<(), Error> {
        let Some(lock) = &mut self.session_lock else {
            return Ok(());
        };
        if matches!(lock.phase, LockPhase::Unlocking(_) | LockPhase::Unlocked) {
            return Ok(());
        }
        if lock.outputs.iter().any(|(existing, _)| existing == output) {
            return Ok(());
        }
        let (view, scene) = SurfaceScene::start(&mut lock.start);
        self.create_surface(SurfaceSpec::lock(view, scene), |platform, wl, id| {
            platform.lock_role(wl, output, id)
        })?;
        Ok(())
    }

    pub(super) fn lock_output_removed(&mut self, output: &wl_output::WlOutput) {
        let Some(lock) = &mut self.session_lock else {
            return;
        };
        if let Some(index) = lock
            .outputs
            .iter()
            .position(|(existing, _)| existing == output)
        {
            let (_, id) = lock.outputs.remove(index);
            self.surfaces[id].lifecycle.close();
        }
    }

    pub(super) fn update_lock(&mut self) -> Result<LockState, Error> {
        let Some(lock) = &mut self.session_lock else {
            return Ok(LockState::Running);
        };
        if let Some(error) = lock.failure.take() {
            return Err(error);
        }
        match lock.phase {
            LockPhase::Finished => Err(Error::SessionLockEnded),
            LockPhase::Unlocked => {
                lock.handle.0.set(Lifetime::Released);
                Ok(LockState::Released)
            }
            // The reply to the sync request shows that the compositor processed the unlock. A compositor that
            // exits before the reply ends the connection instead.
            LockPhase::Locked if lock.handle.0.get() == Lifetime::ReleaseRequested => {
                lock.proxy.unlock_and_destroy();
                let sync = self.connection.display().sync(&self.qh, UnlockSync);
                lock.phase = LockPhase::Unlocking(sync);
                for surface in &mut self.surfaces {
                    surface.lifecycle.close();
                }
                self.connection.flush().map_err(Error::Flush)?;
                Ok(LockState::Running)
            }
            LockPhase::Requested | LockPhase::Locked | LockPhase::Unlocking(_) => {
                Ok(LockState::Running)
            }
        }
    }
}

delegate_noop!(Platform: ignore ExtSessionLockManagerV1);

impl Dispatch<ExtSessionLockV1, ()> for Platform {
    fn event(
        state: &mut Self,
        _: &ExtSessionLockV1,
        event: ext_session_lock_v1::Event,
        (): &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some(lock) = &mut state.session_lock else {
            return;
        };
        match event {
            ext_session_lock_v1::Event::Locked if matches!(lock.phase, LockPhase::Requested) => {
                lock.phase = LockPhase::Locked;
            }
            ext_session_lock_v1::Event::Finished => lock.phase = LockPhase::Finished,
            _ => {}
        }
    }
}

impl Dispatch<ExtSessionLockSurfaceV1, usize> for Platform {
    fn event(
        state: &mut Self,
        surface: &ExtSessionLockSurfaceV1,
        event: ext_session_lock_surface_v1::Event,
        id: &usize,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let ext_session_lock_surface_v1::Event::Configure {
            serial,
            width,
            height,
        } = event
        {
            // A lock surface cannot be drawn at a size of zero. Such a configure is not acknowledged, and the
            // surface continues with the size of the last configure.
            let (Some(width), Some(height)) = (NonZeroU32::new(width), NonZeroU32::new(height))
            else {
                log::warn!("lock surface {id} was configured with the empty size {width}x{height}");
                return;
            };
            surface.ack_configure(serial);
            let native = &mut state.surfaces[*id];
            native.size = (width.get(), height.get());
            native.lifecycle.configure();
            native.view.invalidate();
        }
    }
}

pub(super) struct UnlockSync;

impl Dispatch<wl_callback::WlCallback, UnlockSync> for Platform {
    fn event(
        state: &mut Self,
        callback: &wl_callback::WlCallback,
        event: wl_callback::Event,
        _: &UnlockSync,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_callback::Event::Done { .. } = event
            && let Some(lock) = &mut state.session_lock
            && let LockPhase::Unlocking(sync) = &lock.phase
            && sync == callback
        {
            lock.phase = LockPhase::Unlocked;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_is_scoped_to_one_attached_lifecycle() -> Result<(), Error> {
        let handle = SessionLock::new();
        handle.unlock();
        assert!(handle.0.get() == Lifetime::Fresh);
        handle.attach()?;
        handle.clone().unlock();
        assert!(handle.0.get() == Lifetime::ReleaseRequested);
        handle.0.set(Lifetime::Released);
        assert!(handle.attach().is_err());
        let next = SessionLock::new();
        next.attach()?;
        assert!(next.0.get() == Lifetime::Attached);
        Ok(())
    }
}
