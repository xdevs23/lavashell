// SPDX-License-Identifier: GPL-3.0-only

use crate::sctk;
use lavashell_core::geometry::Size;
use lavashell_core::output::{Millimetres, MonitorInfo, PhysicalSize};
use lavashell_core::units::Dp;
use lavashell_platform::OutputId;
use sctk::output::{Mode, OutputData, OutputInfo};
use sctk::reexports::client::Proxy;
use sctk::reexports::client::globals::GlobalList;
use sctk::reexports::client::protocol::wl_output;
use std::time::Duration;

const MILLIHERTZ_PER_HERTZ: f64 = 1000.0;

fn output_id_of_global(name: u32) -> OutputId {
    OutputId(u64::from(name))
}

pub(crate) fn output_id(output: &wl_output::WlOutput) -> Option<OutputId> {
    let data = output.data::<OutputData>()?;
    Some(output_id_of_global(data.with_output_info(|info| info.id)))
}

pub(crate) fn advertised_outputs(globals: &GlobalList) -> Vec<OutputId> {
    let output = wl_output::WlOutput::interface().name;
    globals.contents().with_list(|list| {
        (list.iter())
            .filter(|global| global.interface == output)
            .map(|global| output_id_of_global(global.name))
            .collect()
    })
}

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

fn physical_size((width, height): (i32, i32)) -> Option<PhysicalSize> {
    (width > 0 && height > 0).then(|| PhysicalSize {
        width: Millimetres(f64::from(width)),
        height: Millimetres(f64::from(height)),
    })
}

pub(crate) fn monitor_info(info: &OutputInfo) -> Option<MonitorInfo> {
    let (width, height) = logical_size(info.logical_size, &info.modes, info.scale_factor)?;
    Some(MonitorInfo {
        name: info.name.clone(),
        size: Size::new(Dp::new(width.into()), Dp::new(height.into())),
        physical_size: physical_size(info.physical_size),
    })
}

pub(crate) fn refresh_interval(modes: &[Mode]) -> Option<Duration> {
    (modes.iter())
        .filter(|mode| mode.current && mode.refresh_rate > 0)
        .map(|mode| Duration::from_secs_f64(MILLIHERTZ_PER_HERTZ / f64::from(mode.refresh_rate)))
        .min()
}

#[cfg(test)]
mod tests;
