// SPDX-License-Identifier: GPL-3.0-only
use super::Translator;
use crate::app::{monitor_info, output_id, refresh_interval};
use crate::sctk;
use lavashell_core::surface::SurfaceId;
use lavashell_platform::{OutputEvent, OutputId};
use sctk::output::{OutputHandler, OutputState};
use sctk::reexports::client::protocol::{wl_output, wl_surface};
use sctk::reexports::client::{Connection, QueueHandle};

impl Translator {
    pub(super) fn find_output(&self, id: OutputId) -> Option<wl_output::WlOutput> {
        self.outputs
            .outputs()
            .find(|output| output_id(output) == Some(id))
    }

    fn report_output(&self, output: &wl_output::WlOutput) {
        let (Some(id), Some(info)) = (output_id(output), self.outputs.info(output)) else {
            return;
        };
        if let Some(monitor) = monitor_info(&info) {
            self.links.report(OutputEvent::Updated(id, monitor));
        }
        self.links
            .report(OutputEvent::Refresh(id, refresh_interval(&info.modes)));
    }

    pub(super) fn surface_and_output(
        &self,
        wl: &wl_surface::WlSurface,
        output: &wl_output::WlOutput,
    ) -> Option<(SurfaceId, OutputId)> {
        Some((self.find_surface(wl)?, output_id(output)?))
    }
}

impl OutputHandler for Translator {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.outputs
    }

    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, output: wl_output::WlOutput) {
        if let Some(id) = output_id(&output) {
            self.links.report(OutputEvent::Added(id));
        }
        self.report_output(&output);
    }

    fn update_output(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        output: wl_output::WlOutput,
    ) {
        self.report_output(&output);
    }

    fn output_destroyed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        output: wl_output::WlOutput,
    ) {
        if let Some(id) = output_id(&output) {
            self.links.report(OutputEvent::Removed(id));
        }
    }
}
