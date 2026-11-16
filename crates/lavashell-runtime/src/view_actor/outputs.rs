// SPDX-License-Identifier: GPL-3.0-only
use crate::layer_surface::LayerSurface;
use crate::output_records::OutputRecords;
use crate::panels::PanelsPerOutput;
use crate::session_lock::{self, ActiveLock, LockConfiguration, LockStep};
use crate::surface_records::SurfaceRecords;
use lavashell_core::output::MonitorInfo;
use lavashell_core::surface::SurfaceId;
use lavashell_core::view::View;
use lavashell_platform::{
    HostFacts, OutputEvent, OutputId, SessionLockEvent, SurfaceEvent, SurfaceRequest,
};
use std::time::Duration;

pub(super) struct Outputs {
    host: HostFacts,
    records: OutputRecords,
    panels_per_output: Option<PanelsPerOutput<LayerSurface>>,
    session_lock: Option<ActiveLock>,
}

impl Outputs {
    pub(super) fn new(
        host: HostFacts,
        panels_per_output: Option<PanelsPerOutput<LayerSurface>>,
    ) -> Self {
        Self {
            host,
            records: OutputRecords::default(),
            panels_per_output,
            session_lock: None,
        }
    }

    pub(super) fn handle(&mut self, event: OutputEvent) {
        self.records.handle(event);
    }

    pub(super) fn forget_surface(&mut self, surface: SurfaceId) {
        self.records.forget(surface);
    }

    pub(super) fn info(&self, output: OutputId) -> Option<MonitorInfo> {
        self.records.info(output).cloned()
    }

    pub(super) fn reports_presentation(&self) -> bool {
        self.host.reports_presentation
    }

    pub(super) fn shortest_refresh(&self, surface: SurfaceId) -> Option<Duration> {
        self.records.refreshes(surface).min()
    }

    pub(super) fn keeps_run_going(&self) -> bool {
        self.session_lock.is_some() || self.panels_per_output.is_some()
    }

    pub(super) fn take_panels_per_output(&mut self) -> Option<PanelsPerOutput<LayerSurface>> {
        self.panels_per_output.take()
    }

    pub(super) fn put_back_panels_per_output(&mut self, panels: PanelsPerOutput<LayerSurface>) {
        self.panels_per_output = Some(panels);
    }

    pub(super) fn close_surfaces_of(
        &mut self,
        output: OutputId,
        records: &mut SurfaceRecords,
    ) -> Vec<SurfaceRequest> {
        let mut closed = Vec::new();
        if let Some(per_output) = &mut self.panels_per_output {
            closed.extend(per_output.output_removed(output, records));
        }
        if let Some(lock) = &mut self.session_lock
            && let Some(surface) = lock.remove_output(output)
        {
            closed.extend(records.handle(surface, SurfaceEvent::Closed));
        }
        closed
    }

    pub(super) fn start_lock(
        &mut self,
        configuration: LockConfiguration,
    ) -> Result<Vec<OutputId>, session_lock::Error> {
        let lock = ActiveLock::start(configuration, &self.host.outputs)?;
        self.session_lock = Some(lock);
        Ok(self.host.outputs.clone())
    }

    pub(super) fn lock_view_for(&self, output: OutputId) -> Option<View> {
        (self.session_lock.as_ref()).and_then(|lock| lock.view_for(output))
    }

    pub(super) fn lock_surface_opened(&mut self, output: OutputId, surface: SurfaceId) {
        if let Some(lock) = &mut self.session_lock {
            lock.opened(output, surface);
        }
    }

    pub(super) fn handle_lock(&mut self, event: SessionLockEvent) {
        if let Some(lock) = &mut self.session_lock {
            lock.handle(event);
        }
    }

    pub(super) fn lock_step(&mut self) -> Option<Result<LockStep, session_lock::Error>> {
        self.session_lock.as_mut().map(ActiveLock::step)
    }

    pub(super) fn release_lock(&mut self) {
        self.session_lock = None;
    }
}
