// SPDX-License-Identifier: GPL-3.0-only
use crate::color::channel_step;
use crate::test_error::TestError;
use crate::text::{PixelGeometry, TextDestination};
use lavashell_core::prelude::*;
use skia_safe::{
    AlphaType, Canvas, ColorSpace, ColorType, Data, ISize, Image, ImageInfo, Surface, SurfaceProps,
};
use std::num::NonZeroUsize;

pub(crate) const EFFECT_SURFACE: TestSurface = TestSurface::new(96, 48);
pub(crate) const EFFECT_STRIPES: StripedSurface = EFFECT_SURFACE.striped(4);
pub(crate) const WIDE_SURFACE: TestSurface = TestSurface::new(64, 30);

#[derive(Clone, Copy)]
pub(crate) struct TestSurface {
    width: u16,
    height: u16,
}

#[derive(Clone, Copy)]
pub(crate) struct StripedSurface {
    surface: TestSurface,
    stripe_width: u16,
}

impl TestSurface {
    pub(crate) const fn new(width: u16, height: u16) -> Self {
        Self { width, height }
    }

    pub(crate) const fn striped(self, stripe_width: u16) -> StripedSurface {
        StripedSurface {
            surface: self,
            stripe_width,
        }
    }

    pub(crate) fn dimensions(self) -> ISize {
        ISize::new(i32::from(self.width), i32::from(self.height))
    }

    pub(crate) fn size(self) -> Size {
        Size::new(
            Dp::new(f64::from(self.width)),
            Dp::new(f64::from(self.height)),
        )
    }

    pub(crate) fn bounds(self) -> Rect {
        Rect::from_size(self.size())
    }

    pub(crate) fn center(self) -> Point {
        self.bounds().center()
    }

    pub(crate) fn pixel_size(self) -> PixelSize {
        PixelSize::new(u32::from(self.width), u32::from(self.height))
    }

    pub(crate) fn pixel_count(self) -> u64 {
        u64::from(self.width) * u64::from(self.height)
    }

    pub(crate) fn raster(self) -> Result<Surface, TestError> {
        let ISize { width, height } = self.dimensions();
        surface(width, height)
    }

    pub(crate) fn middle_column(self) -> i32 {
        i32::from(self.width / 2)
    }

    pub(crate) fn middle_row(self) -> i32 {
        i32::from(self.height / 2)
    }

    pub(crate) fn pixel_distance_from_center(self, (x, y): (u16, u16)) -> f64 {
        let center = self.center();
        (f64::from(x) + 0.5 - center.x.get()).hypot(f64::from(y) + 0.5 - center.y.get())
    }
}

impl StripedSurface {
    pub(crate) fn stripe_at(self, x: Dp) -> Rect {
        Rect::new(
            x,
            Dp::ZERO,
            Dp::new(f64::from(self.stripe_width)),
            self.surface.size().height,
        )
    }

    pub(crate) fn stripes<C: Copy>(self, [even, odd]: [C; 2]) -> impl Iterator<Item = (Rect, C)> {
        (0..self.surface.width / self.stripe_width).map(move |stripe| {
            let x = Dp::new(f64::from(stripe * self.stripe_width));
            (self.stripe_at(x), if stripe % 2 == 0 { even } else { odd })
        })
    }

    pub(crate) fn fill_stripes(self, d: &mut Drawing<'_>, colors: [Color; 2]) {
        for (stripe, color) in self.stripes(colors) {
            d.fill(stripe, color);
        }
    }

    pub(crate) fn stripe_fills(self, colors: [Color; 2]) -> Vec<DrawCommand> {
        let mut commands = Vec::new();
        let context = DrawContext::new(Viewport::new(self.surface.size(), Density::ONE));
        self.fill_stripes(
            &mut Drawing::new(&mut commands, self.surface.bounds(), &context),
            colors,
        );
        commands
    }
}

pub(crate) fn working_info(
    dimensions: impl Into<ISize>,
    color_type: ColorType,
) -> Result<ImageInfo, TestError> {
    Ok(crate::color::working_image_info(dimensions)?.with_color_type(color_type))
}

pub(crate) fn surface(width: i32, height: i32) -> Result<Surface, TestError> {
    let mut surface = skia_safe::surfaces::raster(
        &working_info((width, height), ColorType::RGBAF16)?,
        None,
        None,
    )
    .ok_or(TestError::RasterSurface)?;
    let _canvas_returned_for_chaining = surface.canvas().clear(skia_safe::Color::TRANSPARENT);
    Ok(surface)
}

#[derive(Debug, PartialEq)]
pub(crate) struct Pixels<P> {
    width: usize,
    pixels: Vec<P>,
}

impl<P: Copy> Pixels<P> {
    fn decoded<const BYTES: usize>(
        info: &ImageInfo,
        bytes: &[u8],
        decode: impl Fn(&[u8; BYTES]) -> P,
    ) -> Self {
        let (pixels, _no_partial_pixel) = bytes.as_chunks::<BYTES>();
        Self {
            width: info.min_row_bytes() / BYTES,
            pixels: pixels.iter().map(decode).collect(),
        }
    }

    pub(crate) fn at(&self, x: u16, y: u16) -> P {
        self.pixels[usize::from(y) * self.width + usize::from(x)]
    }

    pub(crate) fn iter(&self) -> std::slice::Iter<'_, P> {
        self.pixels.iter()
    }
}

impl<P> Pixels<P> {
    pub(crate) fn rows(&self) -> Result<std::slice::Chunks<'_, P>, TestError> {
        let width = NonZeroUsize::new(self.width).ok_or(TestError::Missing("pixel columns"))?;
        Ok(self.pixels.chunks(width.get()))
    }

    pub(crate) fn positioned(&self) -> Result<impl Iterator<Item = ((u16, u16), &P)>, TestError> {
        let rows = self.rows()?;
        let (columns, row_count) = (u16::try_from(self.width)?, u16::try_from(rows.len())?);
        Ok((0..row_count)
            .zip(rows)
            .flat_map(move |(y, row)| (0..columns).zip(row).map(move |(x, pixel)| ((x, y), pixel))))
    }

    pub(crate) fn map<Q>(&self, convert: impl FnMut(&P) -> Q) -> Pixels<Q> {
        Pixels {
            width: self.width,
            pixels: self.pixels.iter().map(convert).collect(),
        }
    }

    pub(crate) fn try_map<Q, E>(
        &self,
        convert: impl FnMut(&P) -> Result<Q, E>,
    ) -> Result<Pixels<Q>, E> {
        Ok(Pixels {
            width: self.width,
            pixels: self.pixels.iter().map(convert).collect::<Result<_, _>>()?,
        })
    }

    pub(crate) fn zipped<'pixels, Q>(
        &'pixels self,
        other: &'pixels Pixels<Q>,
    ) -> Result<Pixels<(&'pixels P, &'pixels Q)>, TestError> {
        if (self.width, self.pixels.len()) != (other.width, other.pixels.len()) {
            return Err(TestError::UnpairedPixels {
                widths: [self.width, other.width],
                counts: [self.pixels.len(), other.pixels.len()],
            });
        }
        Ok(Pixels {
            width: self.width,
            pixels: self.pixels.iter().zip(&other.pixels).collect(),
        })
    }
}

impl<C, const N: usize> Pixels<[C; N]> {
    pub(crate) fn channels(&self) -> &[C] {
        self.pixels.as_flattened()
    }
}

pub(crate) fn channel_differences<const N: usize>(
    pixel: &[u8; N],
    other: &[u8; N],
) -> impl Iterator<Item = u8> {
    pixel.iter().zip(other).map(|(a, b)| a.abs_diff(*b))
}

impl<const N: usize> Pixels<[u8; N]> {
    pub(crate) fn largest_channel_difference(&self, other: &Self) -> Result<u8, TestError> {
        (self.zipped(other)?.iter())
            .flat_map(|(a, b)| channel_differences(a, b))
            .max()
            .ok_or(TestError::Missing("pixels"))
    }
}

impl<const N: usize> Pixels<[f32; N]> {
    pub(crate) fn positions_differing_in_bits(
        &self,
        other: &Self,
    ) -> Result<Vec<(u16, u16)>, TestError> {
        Ok((self.zipped(other)?.positioned()?)
            .filter(|(_, (a, b))| a.map(f32::to_bits) != b.map(f32::to_bits))
            .map(|(position, _)| position)
            .collect())
    }
}

impl<'pixels, P> IntoIterator for &'pixels Pixels<P> {
    type Item = &'pixels P;
    type IntoIter = std::slice::Iter<'pixels, P>;

    fn into_iter(self) -> Self::IntoIter {
        self.pixels.iter()
    }
}

pub(crate) fn srgb_info(dimensions: impl Into<ISize>) -> ImageInfo {
    ImageInfo::new(
        dimensions,
        ColorType::RGBA8888,
        AlphaType::Premul,
        ColorSpace::new_srgb(),
    )
}

pub(crate) fn float_info(
    dimensions: impl Into<ISize>,
    alpha: AlphaType,
    color_space: Option<ColorSpace>,
) -> ImageInfo {
    ImageInfo::new(dimensions, ColorType::RGBAF32, alpha, color_space)
}

pub(crate) fn pixels(surface: &mut Surface) -> Pixels<[u8; 4]> {
    let info = srgb_info(surface.image_info().dimensions());
    decode_rgba8(&info, &pixels_as(surface, &info))
}

pub(crate) fn float_pixels(
    surface: &mut Surface,
    alpha: AlphaType,
    color_space: Option<ColorSpace>,
) -> Pixels<[f32; 4]> {
    let info = float_info(surface.image_info().dimensions(), alpha, color_space);
    read_float_pixels(surface, &info)
}

fn read_float_pixels(surface: &mut Surface, info: &ImageInfo) -> Pixels<[f32; 4]> {
    decode_floats(info, &pixels_as(surface, info))
}

pub(crate) fn image_pixels(image: &Image, alpha: AlphaType) -> Pixels<[u8; 4]> {
    let info = srgb_info(image.dimensions()).with_alpha_type(alpha);
    decode_rgba8(&info, &image_bytes(image, &info))
}

pub(crate) fn image_float_pixels(
    image: &Image,
    alpha: AlphaType,
    color_space: Option<ColorSpace>,
) -> Pixels<[f32; 4]> {
    let info = float_info(image.dimensions(), alpha, color_space);
    decode_floats(&info, &image_bytes(image, &info))
}

pub(crate) fn encoded_pixels(
    encoded: &[u8],
    alpha: AlphaType,
) -> Result<Pixels<[u8; 4]>, TestError> {
    let image = Image::from_encoded(Data::new_copy(encoded)).ok_or(crate::Error::DecodeImage)?;
    Ok(image_pixels(&image, alpha))
}

fn image_bytes(image: &Image, info: &ImageInfo) -> Vec<u8> {
    let mut bytes = vec![0; info.compute_min_byte_size()];
    assert!(image.read_pixels(
        info,
        &mut bytes,
        info.min_row_bytes(),
        (0, 0),
        skia_safe::image::CachingHint::Disallow,
    ));
    bytes
}

pub(crate) fn alpha_pixels(surface: &mut Surface) -> Pixels<u8> {
    let info = ImageInfo::new_a8(surface.image_info().dimensions());
    Pixels::decoded(&info, &pixels_as(surface, &info), |[alpha]: &[u8; 1]| {
        *alpha
    })
}

pub(crate) fn working_pixels(surface: &mut Surface) -> Result<Pixels<[f32; 4]>, TestError> {
    let info = working_info(surface.image_info().dimensions(), ColorType::RGBAF32)?;
    Ok(read_float_pixels(surface, &info))
}

pub(crate) fn linear_float_pixels(surface: &mut Surface, alpha: AlphaType) -> Pixels<[f32; 4]> {
    float_pixels(surface, alpha, Some(ColorSpace::new_srgb_linear()))
}

pub(crate) fn linear_float_pixel(
    surface: &mut Surface,
    x: i32,
    y: i32,
    alpha: AlphaType,
) -> [f32; 4] {
    float_pixel(surface, (x, y), alpha, Some(ColorSpace::new_srgb_linear()))
}

pub(crate) fn float_pixel(
    surface: &mut Surface,
    position: (i32, i32),
    alpha: AlphaType,
    color_space: Option<ColorSpace>,
) -> [f32; 4] {
    let info = float_info((1, 1), alpha, color_space);
    decode_floats(&info, &pixels_at(surface, &info, position)).at(0, 0)
}

fn pixels_as(surface: &mut Surface, info: &ImageInfo) -> Vec<u8> {
    pixels_at(surface, info, (0, 0))
}

fn pixels_at(surface: &mut Surface, info: &ImageInfo, origin: (i32, i32)) -> Vec<u8> {
    let mut bytes = vec![0; info.compute_min_byte_size()];
    assert!(surface.read_pixels(info, &mut bytes, info.min_row_bytes(), origin));
    bytes
}

pub(crate) fn drawn_pixels(
    surface: TestSurface,
    color_type: ColorType,
    alpha: AlphaType,
    props: Option<&SurfaceProps>,
    backdrop: skia_safe::Color,
    paint: impl FnOnce(&Canvas) -> Result<(), TestError>,
) -> Result<Pixels<[u8; 4]>, TestError> {
    let info = ImageInfo::new(
        surface.dimensions(),
        color_type,
        alpha,
        ColorSpace::new_srgb(),
    );
    Ok(pixels(&mut drawn_surface(&info, props, backdrop, paint)?))
}

pub(crate) fn drawn_surface(
    info: &ImageInfo,
    props: Option<&SurfaceProps>,
    backdrop: skia_safe::Color,
    paint: impl FnOnce(&Canvas) -> Result<(), TestError>,
) -> Result<Surface, TestError> {
    let mut surface =
        skia_safe::surfaces::raster(info, None, props).ok_or(TestError::RasterSurface)?;
    paint(surface.canvas().clear(backdrop))?;
    Ok(surface)
}

pub(crate) fn lcd_destination() -> TextDestination {
    TextDestination {
        opaque: true,
        geometry: PixelGeometry::Rgb,
    }
}

pub(crate) fn has_colored_fringe(image: &Pixels<[u8; 4]>) -> bool {
    image
        .iter()
        .any(|pixel| pixel[0] != pixel[1] || pixel[1] != pixel[2])
}

pub(crate) fn unpremultiplied_pixel(surface: &mut Surface, x: i32, y: i32) -> [u8; 4] {
    rgba8_pixel(surface, (x, y), AlphaType::Unpremul)
}

pub(crate) fn rgba8_pixel(
    surface: &mut Surface,
    position: (i32, i32),
    alpha: AlphaType,
) -> [u8; 4] {
    let info = ImageInfo::new((1, 1), ColorType::RGBA8888, alpha, None);
    decode_rgba8(&info, &pixels_at(surface, &info, position)).at(0, 0)
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

fn decode_rgba8(info: &ImageInfo, bytes: &[u8]) -> Pixels<[u8; 4]> {
    Pixels::decoded(info, bytes, |pixel: &[u8; 4]| *pixel)
}

fn decode_floats(info: &ImageInfo, bytes: &[u8]) -> Pixels<[f32; 4]> {
    Pixels::decoded(info, bytes, |pixel: &[u8; 16]| floats(pixel))
}

fn floats<const N: usize>(bytes: &[u8]) -> [f32; N] {
    let (numbers, _no_partial_number) = bytes.as_chunks::<4>();
    std::array::from_fn(|index| f32::from_ne_bytes(numbers[index]))
}

pub(crate) fn pattern(surface: TestSurface) -> Result<Image, TestError> {
    let rows = u8::try_from(surface.height)?;
    let columns = u8::try_from(surface.width)?;
    let pixels: Vec<u8> = (0..rows)
        .flat_map(|y| {
            (0..columns)
                .flat_map(move |x| [x.wrapping_mul(3), y.wrapping_mul(5), x.wrapping_mul(y), 255])
        })
        .collect();
    let info = ImageInfo::new(
        surface.dimensions(),
        ColorType::RGBA8888,
        AlphaType::Premul,
        None,
    );
    let row_bytes = usize::from(columns) * 4;
    skia_safe::images::raster_from_data(&info, Data::new_copy(&pixels), row_bytes)
        .ok_or(TestError::Missing("image made from pixels"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zeroes(width: usize, pixel_count: usize) -> Pixels<u8> {
        Pixels {
            width,
            pixels: vec![0; pixel_count],
        }
    }

    fn last_position(pixels: &Pixels<u8>) -> Result<Option<(u16, u16)>, TestError> {
        Ok(pixels.positioned()?.last().map(|(position, _)| position))
    }

    #[test]
    fn rows_and_positioned_return_an_error_for_a_buffer_without_columns() {
        let without_columns = zeroes(0, 0);
        assert!(matches!(without_columns.rows(), Err(TestError::Missing(_))));
        assert!(matches!(
            without_columns.positioned(),
            Err(TestError::Missing(_))
        ));
    }

    #[test]
    fn positioned_ends_at_the_last_position_of_a_buffer_as_wide_as_u16_allows()
    -> Result<(), TestError> {
        let widest = usize::from(u16::MAX);
        assert_eq!(
            last_position(&zeroes(widest, widest))?,
            Some((u16::MAX - 1, 0))
        );
        Ok(())
    }

    #[test]
    fn positioned_ends_at_the_last_position_of_a_buffer_as_tall_as_u16_allows()
    -> Result<(), TestError> {
        let tallest = usize::from(u16::MAX);
        assert_eq!(last_position(&zeroes(1, tallest))?, Some((0, u16::MAX - 1)));
        Ok(())
    }

    #[test]
    fn positioned_returns_an_error_beyond_the_range_of_u16() {
        let beyond = usize::from(u16::MAX) + 1;
        assert!(matches!(
            zeroes(beyond, beyond).positioned(),
            Err(TestError::Integer(_))
        ));
        assert!(matches!(
            zeroes(1, beyond).positioned(),
            Err(TestError::Integer(_))
        ));
    }
}
