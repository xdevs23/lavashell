// SPDX-License-Identifier: GPL-3.0-only
use super::SurfaceTarget;

impl SurfaceTarget {
    /// The premultiplied sRGB bytes of the image that the frames of the target are painted into, or `None` before the
    /// first frame.
    pub(crate) fn painted_pixels(&mut self) -> Option<Vec<u8>> {
        let working = self.chain.as_mut()?.working.as_mut()?;
        Some(crate::test_surfaces::pixels(working))
    }
}
