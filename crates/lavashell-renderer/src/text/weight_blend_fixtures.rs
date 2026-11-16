// SPDX-License-Identifier: GPL-3.0-only
use crate::test_error::TestError;
use crate::test_surfaces::Pixels;
use lavashell_core::prelude::*;

pub(crate) const ONE_LINE: Rect =
    Rect::new(Dp::new(7.0), Dp::new(2.0), Dp::new(280.0), Dp::new(40.0));

pub(crate) const LIGHTER_WEIGHT: f64 = 400.0;

pub(crate) const HEAVIER_WEIGHT: f64 = 700.0;

pub(crate) const BETWEEN_WEIGHT: f64 = 475.0;

pub(crate) fn shares_between() -> [f64; 2] {
    let heavier = (BETWEEN_WEIGHT - LIGHTER_WEIGHT) / (HEAVIER_WEIGHT - LIGHTER_WEIGHT);
    [1.0 - heavier, heavier]
}

pub(crate) fn largest_deviation(
    actual: &Pixels<[u8; 4]>,
    expected: &Pixels<[f64; 4]>,
) -> Result<f64, TestError> {
    Ok((actual.zipped(expected)?.iter())
        .flat_map(|(actual, expected)| actual.iter().zip(*expected))
        .map(|(actual, expected)| (f64::from(*actual) - expected).abs())
        .fold(0.0, f64::max))
}

pub(crate) fn faces_in_shares<T: Copy + Into<f64>>(
    backdrop: &Pixels<[T; 4]>,
    [lighter, heavier]: [&Pixels<[T; 4]>; 2],
    [lighter_share, heavier_share]: [f64; 2],
) -> Result<Pixels<[f64; 4]>, TestError> {
    let faces = lighter.zipped(heavier)?;
    Ok(backdrop
        .zipped(&faces)?
        .map(|&(backdrop, &(lighter, heavier))| {
            std::array::from_fn(|channel| {
                let [backdrop, lighter, heavier]: [f64; 3] =
                    [backdrop, lighter, heavier].map(|pixel| pixel[channel].into());
                backdrop
                    + lighter_share * (lighter - backdrop)
                    + heavier_share * (heavier - backdrop)
            })
        }))
}
