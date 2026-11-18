// SPDX-License-Identifier: GPL-3.0-only
use super::SurfaceTarget;
use lavashell_renderer::test_surfaces::Pixels;

impl SurfaceTarget {
    pub(crate) fn painted_pixels(&mut self) -> Option<Pixels<[u8; 4]>> {
        let working = self.chain.as_mut()?.working.as_mut()?;
        Some(lavashell_renderer::test_surfaces::pixels(working))
    }
}
