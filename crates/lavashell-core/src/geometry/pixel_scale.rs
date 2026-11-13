// SPDX-License-Identifier: GPL-3.0-only
use super::{PixelSize, Point, Viewport};
use crate::scene::Affine;
use crate::units::{Density, Dp, NonNegative, NumberError, Positive, Px};
use num_traits::ToPrimitive;

/// The number of output pixels covered by one viewport pixel, across and down. Two scales are equal
/// when their ratios have the same bits.
#[derive(Clone, Copy, Debug)]
pub struct PixelScale {
    x: AxisScale,
    y: AxisScale,
}

/// A ratio of two positive lengths in pixels. Both are stored so that the viewport length passed to
/// `PixelScale::filling` maps to exactly the output length.
#[derive(Clone, Copy, Debug)]
struct AxisScale {
    output_length: Positive,
    viewport_length: Positive,
}

impl AxisScale {
    const ONE: Self = Self {
        output_length: Positive::ONE,
        viewport_length: Positive::ONE,
    };

    fn filling(viewport_length: Px, output_length: u32) -> Result<Self, NumberError> {
        Ok(Self {
            output_length: Positive::try_new(f64::from(output_length))?,
            viewport_length: Positive::try_new(viewport_length.get())?,
        })
    }

    fn ratio(self) -> f64 {
        self.output_length.get() / self.viewport_length.get()
    }

    fn to_output(self, length: Px) -> Px {
        Px::new(length.get() / self.viewport_length.get() * self.output_length.get())
    }

    fn to_viewport(self, length: Px) -> Px {
        Px::new(length.get() / self.output_length.get() * self.viewport_length.get())
    }
}

impl PixelScale {
    pub const SQUARE: Self = Self {
        x: AxisScale::ONE,
        y: AxisScale::ONE,
    };

    /// The scale at which `viewport` covers exactly `output`.
    ///
    /// # Errors
    ///
    /// Fails when a side of the viewport or of the output is not finite or not above zero.
    pub fn filling(viewport: Viewport, output: PixelSize) -> Result<Self, NumberError> {
        let density = viewport.density;
        Ok(Self {
            x: AxisScale::filling(density.to_px(viewport.size.width), output.width)?,
            y: AxisScale::filling(density.to_px(viewport.size.height), output.height)?,
        })
    }

    /// Maps dp at `density` to output pixels.
    #[must_use]
    pub fn dp_to_output(self, density: Density) -> Affine {
        Affine::scale(
            density.get() * self.x.ratio(),
            density.get() * self.y.ratio(),
        )
    }

    /// The size in whole output pixels, rounded up, of a buffer that covers `viewport` at this scale. For the
    /// viewport passed to [`PixelScale::filling`], it is exactly the output. Another viewport can come out one pixel
    /// larger where its exact size is a whole number of pixels.
    ///
    /// # Errors
    ///
    /// Fails when a side of the viewport is negative, not finite or too long for a buffer.
    pub fn buffer_size(self, viewport: Viewport) -> Result<PixelSize, NumberError> {
        let pixels = |axis: AxisScale, length: Dp| {
            round_up_to_buffer_length(axis.to_output(viewport.density.to_px(length)))
        };
        Ok(PixelSize::new(
            pixels(self.x, viewport.size.width)?,
            pixels(self.y, viewport.size.height)?,
        ))
    }

    /// The dp point in `viewport` shown at output pixel `x`, `y`.
    #[must_use]
    pub fn point(self, viewport: Viewport, x: Px, y: Px) -> Point {
        Point::from_px(
            self.x.to_viewport(x),
            self.y.to_viewport(y),
            viewport.density,
        )
    }
}

impl PartialEq for PixelScale {
    fn eq(&self, other: &Self) -> bool {
        let bits = |scale: &Self| [scale.x.ratio(), scale.y.ratio()].map(f64::to_bits);
        bits(self) == bits(other)
    }
}

impl Eq for PixelScale {}

/// The whole number of pixels, rounded up, that a buffer needs for `length`.
///
/// # Errors
///
/// Fails when `length` is negative, not finite or too long for a buffer.
fn round_up_to_buffer_length(length: Px) -> Result<u32, NumberError> {
    let pixels = NonNegative::try_new(length.get())?.get().ceil();
    pixels.to_u32().ok_or(NumberError::OutOfRange)
}

#[cfg(test)]
mod tests;
