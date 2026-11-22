// SPDX-License-Identifier: GPL-3.0-only
use lavashell_error_chain::next_step::REPORT_THE_FAILURE;
use std::fmt;

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
