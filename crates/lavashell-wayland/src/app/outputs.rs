// SPDX-License-Identifier: GPL-3.0-only
//! Translates the outputs reported by the compositor, and the outputs entered by its surfaces, into output events for
//! the view.

use crate::translator::Translator;
use lavashell_core::geometry::Size;
use lavashell_core::output::{Millimetres, MonitorInfo, PhysicalSize};
use lavashell_core::surface::SurfaceId;
use lavashell_platform::{OutputEvent, OutputId};
use sctk::output::{Mode, OutputData, OutputInfo};
use sctk::reexports::client::Proxy;
use sctk::reexports::client::globals::GlobalList;
use sctk::reexports::client::protocol::{wl_output, wl_surface};
use smithay_client_toolkit as sctk;
use std::time::Duration;

/// `wl_output` sends the refresh rate of a mode in mHz.
const MILLIHERTZ_PER_HERTZ: f64 = 1000.0;

/// The id of the output with the global `name`: the name given by the compositor to the output global, never given to
/// another output.
fn output_id_of_global(name: u32) -> OutputId {
    OutputId(u64::from(name))
}

/// The id of `output`. The info of a bound output contains the name of the output global.
pub(super) fn output_id(output: &wl_output::WlOutput) -> Option<OutputId> {
    let data = output.data::<OutputData>()?;
    Some(output_id_of_global(data.with_output_info(|info| info.id)))
}

/// The ids of the outputs advertised by the compositor in `globals`, in the order of the advertisements.
pub(crate) fn advertised_outputs(globals: &GlobalList) -> Vec<OutputId> {
    let output = wl_output::WlOutput::interface().name;
    globals.contents().with_list(|list| {
        (list.iter())
            .filter(|global| global.interface == output)
            .map(|global| output_id_of_global(global.name))
            .collect()
    })
}

/// The size in logical pixels of an output: `logical`, or the current mode in `modes` divided by `scale_factor`.
/// `None` when the output reports neither.
fn logical_size(
    logical: Option<(i32, i32)>,
    modes: &[Mode],
    scale_factor: i32,
) -> Option<(i32, i32)> {
    logical.or_else(|| {
        let mode = modes.iter().find(|mode| mode.current)?;
        let scale = scale_factor.max(1);
        Some((mode.dimensions.0 / scale, mode.dimensions.1 / scale))
    })
}

/// The physical size from the width and the height in millimetres, or `None` when one of them is not positive.
/// `wl_output` sends zero for an output without a physical size.
fn physical_size((width, height): (i32, i32)) -> Option<PhysicalSize> {
    (width > 0 && height > 0).then(|| PhysicalSize {
        width: Millimetres(f64::from(width)),
        height: Millimetres(f64::from(height)),
    })
}

/// The geometry of `info`, or `None` while the output reports neither a logical size nor a current mode.
fn monitor_info(info: &OutputInfo) -> Option<MonitorInfo> {
    let (width, height) = logical_size(info.logical_size, &info.modes, info.scale_factor)?;
    Some(MonitorInfo {
        name: info.name.clone(),
        size: Size::new(
            crate::logical::length(width.into()),
            crate::logical::length(height.into()),
        ),
        physical_size: physical_size(info.physical_size),
    })
}

/// The time between two refreshes of the current mode in `modes`, or `None` when the mode does not report a rate.
fn refresh_interval(modes: &[Mode]) -> Option<Duration> {
    (modes.iter())
        .filter(|mode| mode.current && mode.refresh_rate > 0)
        .map(|mode| Duration::from_secs_f64(MILLIHERTZ_PER_HERTZ / f64::from(mode.refresh_rate)))
        .min()
}

impl Translator {
    /// The output with the id `id`, or `None` once the compositor removed it.
    pub(super) fn find_output(&self, id: OutputId) -> Option<wl_output::WlOutput> {
        self.outputs
            .outputs()
            .find(|output| output_id(output) == Some(id))
    }

    /// Reports to the view that the compositor added `output`, and then the geometry and the refresh interval of it.
    pub(super) fn add_output(&self, output: &wl_output::WlOutput) {
        if let Some(id) = output_id(output) {
            self.links.report(OutputEvent::Added(id));
        }
        self.report_output(output);
    }

    /// Reports the geometry and the refresh interval of `output` to the view.
    pub(super) fn report_output(&self, output: &wl_output::WlOutput) {
        let (Some(id), Some(info)) = (output_id(output), self.outputs.info(output)) else {
            return;
        };
        if let Some(monitor) = monitor_info(&info) {
            self.links.report(OutputEvent::Updated(id, monitor));
        }
        self.links
            .report(OutputEvent::Refresh(id, refresh_interval(&info.modes)));
    }

    /// Reports to the view that the compositor removed `output`.
    pub(super) fn remove_output(&self, output: &wl_output::WlOutput) {
        if let Some(id) = output_id(output) {
            self.links.report(OutputEvent::Removed(id));
        }
    }

    /// Reports to the view that the surface `wl` entered `output`.
    pub(super) fn report_surface_entered(
        &self,
        wl: &wl_surface::WlSurface,
        output: &wl_output::WlOutput,
    ) {
        if let Some((surface, output)) = self.surface_and_output(wl, output) {
            self.links.report(OutputEvent::Entered(output, surface));
        }
    }

    /// Reports to the view that the surface `wl` left `output`.
    pub(super) fn report_surface_left(
        &self,
        wl: &wl_surface::WlSurface,
        output: &wl_output::WlOutput,
    ) {
        if let Some((surface, output)) = self.surface_and_output(wl, output) {
            self.links.report(OutputEvent::Left(output, surface));
        }
    }

    /// The ids of the surface `wl` and of `output`, or `None` when the platform does not know one of them.
    fn surface_and_output(
        &self,
        wl: &wl_surface::WlSurface,
        output: &wl_output::WlOutput,
    ) -> Option<(SurfaceId, OutputId)> {
        Some((self.find_surface(wl)?, output_id(output)?))
    }
}

#[cfg(test)]
mod tests;
