// SPDX-License-Identifier: GPL-3.0-only
use super::SurfaceTarget;

impl SurfaceTarget {
    pub(crate) fn painted_pixels(&mut self) -> Option<Vec<u8>> {
        let working = self.chain.as_mut()?.working.as_mut()?;
        Some(crate::test_surfaces::pixels(working))
    }
}
