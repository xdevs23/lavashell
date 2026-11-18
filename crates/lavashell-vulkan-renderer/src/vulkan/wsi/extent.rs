// SPDX-License-Identifier: GPL-3.0-only
use super::SurfaceTarget;
use crate::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ExtentChange {
    Kept,
    Changed,
}

impl SurfaceTarget {
    pub(super) fn extent_change(&self) -> Result<ExtentChange, Error> {
        let Some(chain) = self.chain.as_ref().filter(|_| self.transform.is_some()) else {
            return Ok(ExtentChange::Kept);
        };
        let reported = (self.surface.capabilities(&self.device))
            .map_err(Error::QuerySurfaceCapabilities)?
            .current_extent;
        let undefined = reported.width == u32::MAX || reported.height == u32::MAX;
        if undefined || reported == chain.swapchain.request().extent {
            Ok(ExtentChange::Kept)
        } else {
            Ok(ExtentChange::Changed)
        }
    }
}
