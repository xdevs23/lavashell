// SPDX-License-Identifier: GPL-3.0-only
use super::{PixelSize, Point, Viewport};
use crate::units::{Dp, NonNegative, NumberError, Positive, Px};
use num_traits::ToPrimitive;

#[derive(Clone, Copy, Debug)]
pub struct PixelScale {
    x: AxisScale,
    y: AxisScale,
}

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

    pub fn filling(viewport: Viewport, output: PixelSize) -> Result<Self, NumberError> {
        let density = viewport.density;
        Ok(Self {
            x: AxisScale::filling(density.to_px(viewport.size.width), output.width)?,
            y: AxisScale::filling(density.to_px(viewport.size.height), output.height)?,
        })
    }

    pub fn buffer_size(self, viewport: Viewport) -> Result<PixelSize, NumberError> {
        let pixels = |axis: AxisScale, length: Dp| {
            round_up_to_buffer_length(axis.to_output(viewport.density.to_px(length)))
        };
        Ok(PixelSize::new(
            pixels(self.x, viewport.size.width)?,
            pixels(self.y, viewport.size.height)?,
        ))
    }

    #[must_use]
    pub fn point(self, viewport: Viewport, x: Px, y: Px) -> Point {
        Point::from_px(
            self.x.to_viewport(x),
            self.y.to_viewport(y),
            viewport.density,
        )
    }
}

fn round_up_to_buffer_length(length: Px) -> Result<u32, NumberError> {
    let pixels = NonNegative::try_new(length.get())?.get().ceil();
    pixels.to_u32().ok_or(NumberError::OutOfRange)
}

#[cfg(test)]
mod tests;
