// SPDX-License-Identifier: GPL-3.0-only
use std::fmt;

pub(crate) const REPORT_THE_FAILURE: &str = "report the failure with this log";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Fault {
    DefectInThisBuild,
    OutsideThisBuild,
}

pub(crate) struct DefectNotice;

impl fmt::Display for DefectNotice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "This is a defect in this build of the renderer. The next step is to {REPORT_THE_FAILURE}"
        )
    }
}

#[cfg(test)]
mod tests;
