// SPDX-License-Identifier: GPL-3.0-only
//! Creates a group of layer surfaces for each output that the compositor announces, and closes the group when the
//! compositor removes the output.

use super::creation::NativeSpec;
use super::outputs::output_id;
use super::{App, Platform};
use crate::Error;
use crate::surface::LayerSurface;
use lavashell_core::output::MonitorInfo;
use lavashell_core::reactive::State;
use lavashell_core::surface::SurfaceId;
use lavashell_core::view::View;
use lavashell_platform::{OutputId, SurfaceEvent};
use sctk::reexports::client::protocol::wl_output;
use sctk::reexports::protocols_wlr::layer_shell::v1::client::zwlr_layer_shell_v1::ZwlrLayerShellV1;
use smithay_client_toolkit as sctk;
use std::{cell::RefCell, rc::Rc};

type Factory = Box<dyn FnMut(State<MonitorInfo>) -> Vec<(LayerSurface, View)>>;

pub(super) struct Configuration(pub Factory);

struct Group {
    output: OutputId,
    info: State<MonitorInfo>,
    surfaces: Vec<SurfaceId>,
}

pub(super) struct Runtime {
    factory: Rc<RefCell<Factory>>,
    groups: Vec<Group>,
    failure: Option<Error>,
}

impl Runtime {
    pub(super) fn new(
        config: Configuration,
        shell: Option<&ZwlrLayerShellV1>,
    ) -> Result<Self, Error> {
        if shell.is_none() {
            return Err(Error::NoLayerShell);
        }
        Ok(Self {
            factory: Rc::new(RefCell::new(config.0)),
            groups: Vec::new(),
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
        self.output_layers = Some(Configuration(Box::new(factory)));
        Ok(())
    }
}

impl Platform {
    pub(super) fn layers_output_changed(&mut self, output: &wl_output::WlOutput) {
        if let Err(error) = self.add_output_layers(output)
            && let Some(runtime) = &mut self.output_layers
        {
            runtime.failure = Some(error);
        }
    }

    fn add_output_layers(&mut self, output: &wl_output::WlOutput) -> Result<(), Error> {
        let Some(runtime) = &mut self.output_layers else {
            return Ok(());
        };
        let Some(id) = output_id(output) else {
            return Ok(());
        };
        let Some(info) = self.output_records.info(id).cloned() else {
            return Ok(());
        };
        if let Some(group) = runtime.groups.iter_mut().find(|group| group.output == id) {
            group.info.set(info);
            return Ok(());
        }
        let info = State::new(info);
        let requests = (runtime.factory.clone().borrow_mut())(info.clone());
        let mut surfaces = Vec::new();
        for (options, mut view) in requests {
            let context = self.layer_context(&options, &mut view)?;
            let spec = NativeSpec::layer(&options, view, context)?;
            self.create_surface(spec, |platform, wl, surface| {
                surfaces.push(surface);
                platform.layer_role(wl, Some(output), &options, context, surface)
            })?;
        }
        if let Some(runtime) = &mut self.output_layers {
            runtime.groups.push(Group {
                output: id,
                info,
                surfaces,
            });
        }
        Ok(())
    }

    pub(super) fn layers_output_removed(&mut self, output: &wl_output::WlOutput) {
        let (Some(runtime), Some(id)) = (&mut self.output_layers, output_id(output)) else {
            return;
        };
        if let Some(index) = runtime.groups.iter().position(|group| group.output == id) {
            for surface in runtime.groups.remove(index).surfaces {
                self.surface_event(surface, SurfaceEvent::Closed);
            }
        }
    }
}
