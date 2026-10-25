// SPDX-License-Identifier: GPL-3.0-only
use crate::color::channel_step;
use crate::test_error::TestError;
use skia_safe::{AlphaType, ColorType, Data, ImageInfo, Surface};

pub(crate) fn surface(width: i32, height: i32) -> Result<Surface, TestError> {
    let mut surface = skia_safe::surfaces::raster(
        &ImageInfo::new(
            (width, height),
            ColorType::RGBAF16,
            AlphaType::Premul,
            crate::color::working_color_space().ok_or(crate::Error::WorkingColorSpace)?,
        ),
        None,
        None,
    )
    .ok_or(TestError::RasterSurface)?;
    let _chained_canvas = surface.canvas().clear(skia_safe::Color::TRANSPARENT);
    Ok(surface)
}

pub(crate) fn pixels(surface: &mut Surface) -> Vec<u8> {
    let info = ImageInfo::new(
        surface.image_info().dimensions(),
        ColorType::RGBA8888,
        AlphaType::Premul,
        skia_safe::ColorSpace::new_srgb(),
    );
    let mut bytes = vec![0; info.compute_min_byte_size()];
    assert!(surface.read_pixels(&info, &mut bytes, info.min_row_bytes(), (0, 0)));
    bytes
}

/// The rounding error that a surface adds to the color channels of a pixel each time the pixel is stored in it.
#[derive(Clone, Copy)]
pub(crate) struct ChannelRounding {
    /// One step of the coarsest channel of the surface.
    step: f64,
}

impl ChannelRounding {
    pub fn of(surface: &Surface) -> Result<Self, TestError> {
        let step = channel_step(surface.image_info().color_type())
            .ok_or(TestError::Missing("color channel"))?;
        Ok(Self { step })
    }

    /// The largest distance between a channel read from the surface and its exact value `expected` after the pixel
    /// was stored `stores` times. Each store adds one step: half a step for the rounding of the stored value, and the
    /// rest for the conversion into the gamut in which the channel is read. `channel_step` gives the step for values
    /// up to one. The step of a float channel grows with the value above one, so the step is scaled by `expected`.
    pub fn after(self, stores: u8, expected: f64) -> f64 {
        f64::from(stores) * self.step * expected.abs().max(1.0)
    }
}

/// The first `N` `f32` numbers in `bytes`, read in the native byte order of the machine.
pub(crate) fn floats<const N: usize>(bytes: &[u8]) -> [f32; N] {
    let (numbers, _) = bytes.as_chunks::<4>();
    std::array::from_fn(|index| f32::from_ne_bytes(numbers[index]))
}

pub(crate) fn pattern(width: i32, height: i32) -> Result<skia_safe::Image, TestError> {
    let rows = u8::try_from(height)?;
    let columns = u8::try_from(width)?;
    let pixels: Vec<u8> = (0..rows)
        .flat_map(|y| {
            (0..columns)
                .flat_map(move |x| [x.wrapping_mul(3), y.wrapping_mul(5), x.wrapping_mul(y), 255])
        })
        .collect();
    let info = ImageInfo::new(
        (width, height),
        ColorType::RGBA8888,
        AlphaType::Premul,
        None,
    );
    let row_bytes = usize::from(columns) * 4;
    skia_safe::images::raster_from_data(&info, Data::new_copy(&pixels), row_bytes)
        .ok_or(TestError::Missing("image made from pixels"))
}
