// SPDX-License-Identifier: GPL-3.0-only
use super::SurfaceTarget;
use crate::test_surfaces::Pixels;

impl SurfaceTarget {
    pub(crate) fn painted_pixels(&mut self) -> Option<Pixels<[u8; 4]>> {
        let working = self.chain.as_mut()?.working.as_mut()?;
        Some(crate::test_surfaces::pixels(working))
    }
}
