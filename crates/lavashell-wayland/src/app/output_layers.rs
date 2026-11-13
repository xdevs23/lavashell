// SPDX-License-Identifier: GPL-3.0-only
use super::App;
use crate::SetupError;
use lavashell_core::output::MonitorInfo;
use lavashell_core::reactive::State;
use lavashell_core::view::View;
use lavashell_runtime::{LayerSurface, PanelsPerOutput};

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
    ) -> Result<(), SetupError> {
        if self.output_layers.is_some() {
            return Err(SetupError::OutputLayersConfigured);
        }
        self.output_layers = Some(PanelsPerOutput::new(factory));
        Ok(())
    }
}
