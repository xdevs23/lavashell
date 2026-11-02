// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::output::MonitorInfo;
use lavashell_core::surface::SurfaceId;
use std::time::Duration;

/// Identifies an output of the host, such as a monitor. A platform gives an output the same id for as long as the host
/// has the output, and never gives the id to another output.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OutputId(pub u64);

/// What the host did with an output, or with the surfaces shown on it.
#[derive(Clone, Debug, PartialEq)]
pub enum OutputEvent {
    /// The geometry of the output, sent when the output appears and each time the geometry changes.
    Updated(OutputId, MonitorInfo),
    Removed(OutputId),
    /// The time between two refreshes of the output, or `None` when the host does not report one.
    Refresh(OutputId, Option<Duration>),
    /// The surface is shown on the output, in whole or in part.
    Entered(OutputId, SurfaceId),
    Left(OutputId, SurfaceId),
}
