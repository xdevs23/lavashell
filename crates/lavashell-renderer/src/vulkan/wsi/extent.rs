// SPDX-License-Identifier: GPL-3.0-only
use super::SurfaceTarget;
use crate::Error;

/// Whether the swapchain still has the extent that the surface reports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Extent {
    Kept,
    Changed,
}

impl SurfaceTarget {
    /// Compares the extent of the swapchain with the current extent reported by the surface. A target with an
    /// explicit transform does not recreate its swapchain when Vulkan reports `VK_SUBOPTIMAL_KHR`. This comparison is
    /// therefore the only way such a target detects that the host resized the surface. Every other target returns
    /// `Kept`.
    pub(super) fn surface_extent(&self) -> Result<Extent, Error> {
        let Some(chain) = self.chain.as_ref().filter(|_| self.transform.is_some()) else {
            return Ok(Extent::Kept);
        };
        let reported = (self.surface.capabilities(&self.device))
            .map_err(Error::SurfaceQuery)?
            .current_extent;
        let undefined = reported.width == u32::MAX || reported.height == u32::MAX;
        if undefined || reported == chain.swapchain.request().extent {
            Ok(Extent::Kept)
        } else {
            Ok(Extent::Changed)
        }
    }
}
