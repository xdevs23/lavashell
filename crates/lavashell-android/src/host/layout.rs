// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::geometry::{PixelScale, PixelSize, Point, Rect, Size, Viewport};
use lavashell_core::units::{Density, NumberError, PositiveDp, Px};

/// A size in dp, with both sides finite and above zero, in which an AOSP host lays the app out.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutSize {
    width: PositiveDp,
    height: PositiveDp,
}

impl LayoutSize {
    /// # Errors
    ///
    /// Fails when a side of `size` is not finite or not above zero.
    pub fn try_new(size: Size) -> Result<Self, NumberError> {
        let (width, height) = size.to_positive()?;
        Ok(Self { width, height })
    }

    fn size(self) -> Size {
        Size::new(self.width.get(), self.height.get())
    }
}

/// The viewport in which the runtime lays out the shown surface, and the scale at which its frames fill the Surface
/// of the host.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Layout {
    pub(crate) viewport: Viewport,
    pub(crate) scale: PixelScale,
}

impl Layout {
    /// The layout in `layout_size` at `density` that fills a Surface of `window_size` pixels, or `None` while the
    /// Surface is empty. Without `layout_size`, the layout covers the Surface at `density` with square pixels.
    ///
    /// # Errors
    /// Fails when a side of `layout_size` at `density` is too long or too short for a number of pixels.
    pub(crate) fn fitting(
        window_size: PixelSize,
        density: Density,
        layout_size: Option<LayoutSize>,
    ) -> Result<Option<Self>, NumberError> {
        if window_size.width == 0 || window_size.height == 0 {
            return Ok(None);
        }
        let layout = if let Some(layout_size) = layout_size {
            let viewport = Viewport::new(layout_size.size(), density);
            let scale = PixelScale::filling(viewport, window_size)?;
            Self { viewport, scale }
        } else {
            // The Surface in dp does not always convert back to the same pixels, so `filling` could miss square pixels.
            let px = |pixels: u32| Px::new(f64::from(pixels));
            let size = Size::from_px(px(window_size.width), px(window_size.height), density);
            let viewport = Viewport::new(size, density);
            Self {
                viewport,
                scale: PixelScale::SQUARE,
            }
        };
        Ok(Some(layout))
    }

    /// The dp point shown at the pixel `x`, `y` of the Surface.
    pub(crate) fn point(self, x: Px, y: Px) -> Point {
        self.scale.point(self.viewport, x, y)
    }

    /// Whether `point` lies in the viewport.
    pub(crate) fn contains(self, point: Point) -> bool {
        Rect::from_size(self.viewport.size).contains(point)
    }
}
