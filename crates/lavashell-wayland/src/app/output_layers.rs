// SPDX-License-Identifier: GPL-3.0-only
//! Creates a group of layer surfaces for each output that the compositor announces, and closes the group when the
//! compositor removes the output.

use super::creation::SurfaceSpec;
use super::{
    App, LayerSurface, Platform, Size, State, SurfaceEvent, SurfaceId, View, ZwlrLayerShellV1,
    wl_output,
};
use crate::Error;
use std::{cell::RefCell, rc::Rc};

#[derive(Clone, Debug, PartialEq)]
pub struct MonitorInfo {
    /// `None` when the compositor sends no name for the output.
    pub name: Option<String>,
    pub size: Size,
    pub physical_size_mm: (i32, i32),
}

type Factory = Box<dyn FnMut(State<MonitorInfo>) -> Vec<(LayerSurface, View)>>;

pub(super) struct Configuration(pub Factory);

struct Group {
    output: wl_output::WlOutput,
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
    pub(super) fn layers_output_changed(&mut self, output: wl_output::WlOutput) {
        if let Err(error) = self.add_output_layers(output)
            && let Some(runtime) = &mut self.output_layers
        {
            runtime.failure = Some(error);
        }
    }

    fn add_output_layers(&mut self, output: wl_output::WlOutput) -> Result<(), Error> {
        let Some(runtime) = &mut self.output_layers else {
            return Ok(());
        };
        let Some(info) = self.outputs.info(&output) else {
            return Ok(());
        };
        // An output that reports neither a logical size nor a current mode gets its layers with the first later
        // update that reports one.
        let Some((width, height)) = info.logical_size.or_else(|| {
            let mode = info.modes.iter().find(|mode| mode.current)?;
            let scale = info.scale_factor.max(1);
            Some((mode.dimensions.0 / scale, mode.dimensions.1 / scale))
        }) else {
            return Ok(());
        };
        let info = MonitorInfo {
            name: info.name,
            size: Size::new(
                crate::logical::length(width.into()),
                crate::logical::length(height.into()),
            ),
            physical_size_mm: info.physical_size,
        };
        if let Some(group) = runtime
            .groups
            .iter_mut()
            .find(|group| group.output == output)
        {
            group.info.set(info);
            return Ok(());
        }
        let info = State::new(info);
        let requests = (runtime.factory.clone().borrow_mut())(info.clone());
        let mut surfaces = Vec::new();
        for (options, mut view) in requests {
            let context = self.layer_context(&options, &mut view)?;
            let spec = SurfaceSpec::layer(&options, view, context)?;
            self.create_surface(spec, |platform, wl, id| {
                surfaces.push(id);
                platform.layer_role(wl, Some(&output), &options, context, id)
            })?;
        }
        if let Some(runtime) = &mut self.output_layers {
            runtime.groups.push(Group {
                output,
                info,
                surfaces,
            });
        }
        Ok(())
    }

    pub(super) fn layers_output_removed(&mut self, output: &wl_output::WlOutput) {
        let Some(runtime) = &mut self.output_layers else {
            return;
        };
        if let Some(index) = runtime
            .groups
            .iter()
            .position(|group| &group.output == output)
        {
            for id in runtime.groups.remove(index).surfaces {
                self.surface_event(id, SurfaceEvent::Closed);
            }
        }
    }
}
