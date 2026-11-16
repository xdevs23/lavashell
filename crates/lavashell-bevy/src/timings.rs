// SPDX-License-Identifier: GPL-3.0-only
use crate::Error;
use std::time::Duration;

#[derive(Clone, Copy, Debug)]
pub struct OverlayTiming {
    pub submit: Duration,
    pub host_flush: Duration,
}

#[derive(Debug)]
pub struct OverlayOutcome {
    pub sequence: u64,
    pub result: Result<OverlayTiming, Error>,
}
