// SPDX-License-Identifier: GPL-3.0-only
use crate::{FrameInterval, HostInput, RecordRequests};
use lavashell_actor::Latest;
use std::sync::Arc;
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pace {
    Wake {
        at: Option<Duration>,
        every: Option<FrameInterval>,
    },
    FrameAtSameTime,
    Finished,
}

#[derive(Clone, Default)]
pub struct LatestPace(Arc<Latest<Pace>>);

impl LatestPace {
    pub fn publish(&self, pace: Pace) {
        self.0.publish(pace);
    }

    pub(crate) fn read(&self) -> Option<Pace> {
        self.0.read().as_deref().copied()
    }
}

#[derive(Clone, Default)]
pub struct PacedLinks {
    pub pace: LatestPace,
    pub input: HostInput,
    pub record_requests: RecordRequests,
}
