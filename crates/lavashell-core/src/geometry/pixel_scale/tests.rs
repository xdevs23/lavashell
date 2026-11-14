// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::float_asserts::{assert_f64_eq, assert_f64s_eq};
use crate::geometry::Size;
use crate::test_error::TestResult;
use crate::units::{Density, DpExt};

/// Dots per inch of common displays, from the lowest to the highest density bucket of Android.
const DPIS: [f64; 11] = [
    120.0, 160.0, 213.0, 240.0, 320.0, 400.0, 420.0, 440.0, 480.0, 560.0, 640.0,
];

fn sized(width: Dp, height: Dp, density: Density) -> Viewport {
    Viewport::new(Size::new(width, height), density)
}

#[test]
fn a_scale_fills_a_wider_output_and_the_viewport_covers_exactly_it() -> TestResult {
    let viewport = sized(800.dp(), 480.dp(), Density::ONE);
    let output = PixelSize::new(1280, 480);
    let scale = PixelScale::filling(viewport, output)?;
    assert_eq!(scale.buffer_size(viewport)?, output);
    let dp_to_output = scale.dp_to_output(Density::from_ratio(2.0, 1.0)?);
    assert_f64s_eq(&dp_to_output.linear, &Affine::scale(3.2, 2.0).linear);
    let half = sized(400.dp(), 240.dp(), Density::ONE);
    assert_eq!(scale, PixelScale::filling(half, PixelSize::new(640, 240))?);
    assert_ne!(scale, PixelScale::SQUARE);
    Ok(())
}

#[test]
fn a_viewport_covers_exactly_the_output_its_scale_fills() -> TestResult {
    for dpi in DPIS {
        let density = Density::from_dpi(dpi)?;
        for tenths in (10..20_000).step_by(97) {
            let width = Dp::new(f64::from(tenths) / 10.0);
            let viewport = sized(width, width * 0.6, density);
            for pixels in (1..4000).step_by(31) {
                let output = PixelSize::new(pixels, pixels / 2 + 7);
                let scale = PixelScale::filling(viewport, output)?;
                assert_eq!(
                    scale.buffer_size(viewport)?,
                    output,
                    "{width:?} at {dpi} dpi"
                );
            }
        }
    }
    Ok(())
}

#[test]
fn a_scale_maps_an_output_pixel_back_to_the_dp_point_it_shows() -> TestResult {
    let viewport = sized(400.dp(), 300.dp(), Density::from_dpi(420.0)?);
    let scale = PixelScale::filling(viewport, PixelSize::new(1600, 900))?;
    for ((x, y), (left, top)) in [
        ((0.0, 0.0), (0.0, 0.0)),
        ((800.0, 450.0), (200.0, 150.0)),
        ((1600.0, 900.0), (400.0, 300.0)),
    ] {
        let point = scale.point(viewport, Px::new(x), Px::new(y));
        assert_f64_eq(point.x.get(), left);
        assert_f64_eq(point.y.get(), top);
    }
    let wide = sized(800.dp(), 480.dp(), Density::ONE);
    let point = PixelScale::filling(wide, PixelSize::new(1280, 480))?.point(
        wide,
        Px::new(793.0),
        Px::new(216.0),
    );
    assert_f64_eq(point.x.get(), 495.625);
    assert_f64_eq(point.y.get(), 216.0);
    Ok(())
}

#[test]
fn a_square_scale_maps_each_viewport_pixel_to_one_output_pixel() -> TestResult {
    let viewport = sized(Dp::new(100.5), 20.dp(), Density::from_dpi(420.0)?);
    assert_eq!(
        PixelScale::SQUARE.buffer_size(viewport)?,
        PixelSize::new(264, 53)
    );
    let point = PixelScale::SQUARE.point(viewport, Px::new(42.0), Px::new(7.0));
    assert_f64_eq(point.x.get(), 16.0);
    assert_f64_eq(point.y.get(), 7.0 / 2.625);
    Ok(())
}

#[test]
fn only_finite_sides_above_zero_have_a_scale() {
    let output = PixelSize::new(1280, 480);
    let filling = |width: f64, height: f64, output| {
        PixelScale::filling(sized(Dp::new(width), Dp::new(height), Density::ONE), output).err()
    };
    assert_eq!(
        filling(800.0, 480.0, PixelSize::new(0, 480)),
        Some(NumberError::OutOfRange)
    );
    assert_eq!(filling(800.0, 0.0, output), Some(NumberError::OutOfRange));
    assert_eq!(filling(-800.0, 480.0, output), Some(NumberError::Negative));
    assert_eq!(
        filling(f64::NAN, 480.0, output),
        Some(NumberError::NotFinite)
    );
}

#[test]
fn a_buffer_fails_for_a_negative_or_endless_side_and_for_one_too_long() -> TestResult {
    let buffer =
        |width: f64| PixelScale::SQUARE.buffer_size(sized(Dp::new(width), 1.dp(), Density::ONE));
    assert_eq!(buffer(-0.5), Err(NumberError::Negative));
    assert_eq!(buffer(f64::INFINITY), Err(NumberError::NotFinite));
    let pixel = sized(1.dp(), 1.dp(), Density::ONE);
    let stretched = PixelScale::filling(pixel, PixelSize::new(u32::MAX, 1))?;
    assert_eq!(
        stretched.buffer_size(sized(2.dp(), 1.dp(), Density::ONE)),
        Err(NumberError::OutOfRange)
    );
    Ok(())
}
