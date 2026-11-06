// SPDX-License-Identifier: GPL-3.0-only
use super::creation::NativeSpec;
use super::outputs::output_id;
use super::{Platform, Role, configure_event};
use crate::Error;
use lavashell_core::output::OutputRequest;
use lavashell_core::surface::SurfaceId;
use lavashell_core::view::View;
use lavashell_platform::{Appearance, LockEvent, OutputId, SurfaceEvent};
use lavashell_runtime::{ActiveLock, LockConfiguration, LockStep};
use sctk::reexports::client::globals::GlobalList;
use sctk::reexports::client::protocol::{wl_callback, wl_output};
use sctk::reexports::client::{Connection, Dispatch, QueueHandle, delegate_noop};
use sctk::reexports::protocols::ext::session_lock::v1::client::{
    ext_session_lock_manager_v1::ExtSessionLockManagerV1,
    ext_session_lock_surface_v1::{self, ExtSessionLockSurfaceV1},
    ext_session_lock_v1::{self, ExtSessionLockV1},
};
use smithay_client_toolkit as sctk;
use std::num::NonZeroU32;

pub(super) type LockSurface = ExtSessionLockSurfaceV1;

/// The session lock while the run has one: the lock rules from the runtime and the lock object created by the
/// compositor.
pub(super) struct Runtime {
    proxy: ExtSessionLockV1,
    rules: ActiveLock,
    /// The first failure of a lock surface opened for a new output, which the next lock step returns.
    failure: Option<Error>,
}

impl NativeSpec {
    pub(super) fn lock(view: View) -> Self {
        Self {
            view,
            size: (1, 1),
            appearance: Appearance {
                transparent: false,
                output: OutputRequest::default(),
                hdr_metadata: None,
                background_blur: false,
            },
            shape: None,
            drag_region: None,
            accessible_name: String::new(),
        }
    }
}

fn resolve_output_id(output: &wl_output::WlOutput) -> Result<OutputId, Error> {
    output_id(output).ok_or(Error::OutputWithoutId)
}

/// Starts the lock set by `configuration` on `outputs` and requests it from the compositor through `manager`. The
/// lock rules run first, so a lock that they refuse is never requested.
fn request_lock(
    manager: &ExtSessionLockManagerV1,
    qh: &QueueHandle<Platform>,
    configuration: LockConfiguration,
    outputs: &[OutputId],
) -> Result<Runtime, Error> {
    let rules = ActiveLock::start(configuration, outputs)?;
    let proxy = manager.lock(qh, ());
    manager.destroy();
    Ok(Runtime {
        proxy,
        rules,
        failure: None,
    })
}

fn send_unlock(proxy: &ExtSessionLockV1, connection: &Connection, qh: &QueueHandle<Platform>) {
    proxy.unlock_and_destroy();
    // The reply to the sync request shows that the compositor processed the unlock. A compositor that exits before
    // the reply ends the connection instead.
    let _sync = connection.display().sync(qh, UnlockSync);
}

impl Platform {
    /// Locks the session as set by `configuration`, with a lock surface on every output advertised by the compositor.
    pub(super) fn start_lock(
        &mut self,
        globals: &GlobalList,
        configuration: LockConfiguration,
    ) -> Result<(), Error> {
        let manager: ExtSessionLockManagerV1 = globals
            .bind(&self.qh, 1..=1, ())
            .map_err(Error::BindSessionLock)?;
        let outputs = (self.outputs.outputs())
            .map(|output| resolve_output_id(&output).map(|id| (id, output)))
            .collect::<Result<Vec<_>, _>>()?;
        let ids: Vec<_> = outputs.iter().map(|(id, _)| *id).collect();
        self.session_lock = Some(request_lock(&manager, &self.qh, configuration, &ids)?);
        for (id, output) in &outputs {
            self.add_lock_output(*id, output)?;
        }
        Ok(())
    }

    fn add_lock_output(&mut self, id: OutputId, output: &wl_output::WlOutput) -> Result<(), Error> {
        let Some(lock) = &self.session_lock else {
            return Ok(());
        };
        let Some(view) = lock.rules.view_for(id) else {
            return Ok(());
        };
        let proxy = lock.proxy.clone();
        let surface = self.create_surface(NativeSpec::lock(view), |platform, wl, surface| {
            Ok(Role::Lock(proxy.get_lock_surface(
                wl,
                output,
                &platform.qh,
                surface,
            )))
        })?;
        if let Some(lock) = &mut self.session_lock {
            lock.rules.opened(id, surface);
        }
        Ok(())
    }

    pub(super) fn lock_output_added(&mut self, output: &wl_output::WlOutput) {
        if self.session_lock.is_none() {
            return;
        }
        let added = resolve_output_id(output).and_then(|id| self.add_lock_output(id, output));
        if let Err(error) = added
            && let Some(lock) = &mut self.session_lock
            && lock.failure.is_none()
        {
            lock.failure = Some(error);
        }
    }

    pub(super) fn lock_output_removed(&mut self, output: &wl_output::WlOutput) {
        if let Some(id) = output_id(output)
            && let Some(lock) = &mut self.session_lock
            && let Some(surface) = lock.rules.remove_output(id)
        {
            self.surface_event(surface, SurfaceEvent::Closed);
        }
    }

    /// Runs the lock step of this turn. The unlock closes every surface of the run, and the release ends the run.
    pub(super) fn update_lock(&mut self) -> Result<(), Error> {
        let Some(lock) = &mut self.session_lock else {
            return Ok(());
        };
        if let Some(error) = lock.failure.take() {
            return Err(error);
        }
        match lock.rules.step()? {
            LockStep::Running => {}
            LockStep::Unlock => {
                send_unlock(&lock.proxy, &self.connection, &self.qh);
                let surfaces: Vec<_> = self.surfaces.keys().copied().collect();
                for surface in surfaces {
                    self.surface_event(surface, SurfaceEvent::Closed);
                }
                self.connection.flush().map_err(Error::Flush)?;
            }
            LockStep::Released => {
                self.session_lock = None;
                self.run_end.end();
            }
        }
        Ok(())
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
        let event = match event {
            ext_session_lock_v1::Event::Locked => LockEvent::Locked,
            ext_session_lock_v1::Event::Finished => LockEvent::Finished,
            _ => return,
        };
        if let Some(lock) = &mut state.session_lock {
            lock.rules.handle(event);
        }
    }
}

impl Dispatch<ExtSessionLockSurfaceV1, SurfaceId> for Platform {
    fn event(
        state: &mut Self,
        surface: &ExtSessionLockSurfaceV1,
        event: ext_session_lock_surface_v1::Event,
        id: &SurfaceId,
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
                log::warn!(
                    "lock surface {} was configured with the empty size {width}x{height}",
                    id.0
                );
                return;
            };
            surface.ack_configure(serial);
            let event = configure_event(Some(width.get()), Some(height.get()));
            state.surface_event(*id, event);
        }
    }
}

pub(super) struct UnlockSync;

impl Dispatch<wl_callback::WlCallback, UnlockSync> for Platform {
    fn event(
        state: &mut Self,
        _: &wl_callback::WlCallback,
        event: wl_callback::Event,
        _: &UnlockSync,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_callback::Event::Done { .. } = event
            && let Some(lock) = &mut state.session_lock
        {
            lock.rules.handle(LockEvent::UnlockConfirmed);
        }
    }
}

#[cfg(test)]
mod wire_tests;
