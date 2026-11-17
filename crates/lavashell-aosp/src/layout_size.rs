// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::geometry::{PixelScale, PixelSize, ScaledViewport, Size, Viewport};
use lavashell_core::units::{Density, NumberError, PositiveDp, Px};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutSize {
    width: PositiveDp,
    height: PositiveDp,
}

impl LayoutSize {
    pub fn try_new(size: Size) -> Result<Self, NumberError> {
        let (width, height) = size.to_positive()?;
        Ok(Self { width, height })
    }

    fn size(self) -> Size {
        Size::new(self.width.get(), self.height.get())
    }
}

pub(crate) fn fit_viewport_into_window(
    window_size: PixelSize,
    density: Density,
    layout_size: Option<LayoutSize>,
) -> Result<Option<ScaledViewport>, NumberError> {
    if window_size.width == 0 || window_size.height == 0 {
        return Ok(None);
    }
    let scaled_viewport = match layout_size {
        Some(layout_size) => {
            let viewport = Viewport::new(layout_size.size(), density);
            let scale = PixelScale::filling(viewport, window_size)?;
            ScaledViewport { viewport, scale }
        }
        None => cover_with_square_pixels(window_size, density),
    };
    Ok(Some(scaled_viewport))
}

fn cover_with_square_pixels(window_size: PixelSize, density: Density) -> ScaledViewport {
    let px = |pixels: u32| Px::new(f64::from(pixels));
    let size = Size::from_px(px(window_size.width), px(window_size.height), density);
    ScaledViewport {
        viewport: Viewport::new(size, density),
        scale: PixelScale::SQUARE,
    }
}
