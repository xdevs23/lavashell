// SPDX-License-Identifier: GPL-3.0-only
//! Creates the layer surfaces requested by the runtime on each output, and closes them when the compositor removes
//! the output.

use super::outputs::output_id;
use super::{App, Platform};
use crate::Error;
use crate::surface::LayerSurface;
use lavashell_core::output::MonitorInfo;
use lavashell_core::reactive::State;
use lavashell_core::view::View;
use lavashell_runtime::PanelsPerOutput;
use sctk::reexports::client::protocol::wl_output;
use sctk::reexports::protocols_wlr::layer_shell::v1::client::zwlr_layer_shell_v1::ZwlrLayerShellV1;
use smithay_client_toolkit as sctk;

pub(super) struct Runtime {
    panels: PanelsPerOutput<LayerSurface>,
    failure: Option<Error>,
}

impl Runtime {
    pub(super) fn new(
        panels: PanelsPerOutput<LayerSurface>,
        shell: Option<&ZwlrLayerShellV1>,
    ) -> Result<Self, Error> {
        if shell.is_none() {
            return Err(Error::NoLayerShell);
        }
        Ok(Self {
            panels,
            failure: None,
        })
    }

    pub(super) fn check(&mut self) -> Result<(), Error> {
        self.failure.take().map_or(Ok(()), Err)
    }
}

impl App {
    /// Registers `factory`, which runs once for each output that appears and returns the layer surfaces to create on
    /// that output. The `State<MonitorInfo>` passed to the factory updates when the geometry of the output changes.
    /// The surfaces close when the output is removed.
    ///
    /// # Errors
    ///
    /// Fails when the app has a per-output layer factory already.
    pub fn layers_per_output(
        &mut self,
        factory: impl FnMut(State<MonitorInfo>) -> Vec<(LayerSurface, View)> + 'static,
    ) -> Result<(), Error> {
        if self.output_layers.is_some() {
            return Err(Error::OutputLayersConfigured);
        }
        self.output_layers = Some(PanelsPerOutput::new(factory));
        Ok(())
    }
}

impl Platform {
    pub(super) fn layers_output_changed(&mut self, output: &wl_output::WlOutput) {
        let Some(id) = output_id(output) else {
            return;
        };
        // The runtime is taken out while it opens layers, because opening a layer borrows the whole platform.
        let Some(mut runtime) = self.output_layers.take() else {
            return;
        };
        let info = self.output_records.info(id).cloned();
        let opened = runtime.panels.output_updated(id, info, |options, view| {
            self.create_layer(&options, view, Some(output))
        });
        if let Err(error) = opened {
            runtime.failure = Some(error);
        }
        self.output_layers = Some(runtime);
    }

    pub(super) fn layers_output_removed(&mut self, output: &wl_output::WlOutput) {
        if let (Some(runtime), Some(id)) = (&mut self.output_layers, output_id(output)) {
            let requests = runtime.panels.output_removed(id, &mut self.surface_records);
            self.surface_requests.extend(requests);
        }
    }
}
