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
    let _canvas_returned_for_chaining = surface.canvas().clear(skia_safe::Color::TRANSPARENT);
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

pub(crate) fn unpremultiplied_pixel(surface: &mut Surface, x: i32, y: i32) -> [u8; 4] {
    let info = ImageInfo::new((1, 1), ColorType::RGBA8888, AlphaType::Unpremul, None);
    let mut pixel = [0; 4];
    assert!(surface.read_pixels(&info, &mut pixel, info.min_row_bytes(), (x, y)));
    pixel
}

#[derive(Clone, Copy)]
pub(crate) struct ChannelRounding {
    step: f64,
}

impl ChannelRounding {
    pub fn of(surface: &Surface) -> Result<Self, TestError> {
        let step = channel_step(surface.image_info().color_type())
            .ok_or(TestError::Missing("color channel"))?;
        Ok(Self { step })
    }

    pub fn after(self, stores: u8, expected: f64) -> f64 {
        f64::from(stores) * self.step * expected.abs().max(1.0)
    }
}

pub(crate) fn floats<const N: usize>(bytes: &[u8]) -> [f32; N] {
    let (numbers, _no_partial_number) = bytes.as_chunks::<4>();
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
