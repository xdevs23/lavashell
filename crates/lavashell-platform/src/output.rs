// SPDX-License-Identifier: GPL-3.0-only
use lavashell_actor::{Merge, MergeEffect};
use lavashell_core::output::MonitorInfo;
use lavashell_core::surface::SurfaceId;
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OutputId(pub u64);

#[derive(Clone, Debug, PartialEq)]
pub enum OutputEvent {
    Added(OutputId),
    Updated(OutputId, MonitorInfo),
    Removed(OutputId),
    Refresh(OutputId, Option<Duration>),
    Entered(OutputId, SurfaceId),
    Left(OutputId, SurfaceId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OutputEventKey {
    Geometry(OutputId),
    Refresh(OutputId),
}

impl Merge for OutputEvent {
    type Key = OutputEventKey;

    fn effect(&self) -> MergeEffect<OutputEventKey> {
        match self {
            Self::Updated(output, _) => MergeEffect::Overwrites(OutputEventKey::Geometry(*output)),
            Self::Refresh(output, _) => MergeEffect::Overwrites(OutputEventKey::Refresh(*output)),
            _ => MergeEffect::Appends,
        }
    }
}
