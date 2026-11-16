// SPDX-License-Identifier: GPL-3.0-only
use super::{Corners, RoundedRect};
use crate::geometry::Point;
use crate::numbers::{count_f64, whole_count};
use crate::units::{Dp, NumberError};
use std::f64::consts::{FRAC_PI_2, PI};

impl RoundedRect {
    pub fn outline(&self, device_pixel: Dp) -> Result<Vec<Point>, NumberError> {
        let (left, top) = (self.rect.x, self.rect.y);
        let (right, bottom) = (left + self.rect.width, top + self.rect.height);
        let Corners {
            top_left,
            top_right,
            bottom_right,
            bottom_left,
        } = self.fitted_corners();
        let arcs = [
            (Point::new(left + top_left, top + top_left), top_left, PI),
            (
                Point::new(right - top_right, top + top_right),
                top_right,
                3.0 * FRAC_PI_2,
            ),
            (
                Point::new(right - bottom_right, bottom - bottom_right),
                bottom_right,
                0.0,
            ),
            (
                Point::new(left + bottom_left, bottom - bottom_left),
                bottom_left,
                FRAC_PI_2,
            ),
        ];
        let mut points = Vec::new();
        for (center, radius, start) in arcs {
            let (x, y) = (center.x.get(), center.y.get());
            let radius = radius.get();
            if radius == 0.0 {
                points.push(center);
                continue;
            }
            let rounding = f64::EPSILON / 2.0 * (x.abs().max(y.abs()) + radius);
            let chords = chords_per_quarter_turn(radius, (device_pixel.get() / 2.0).max(rounding))?;
            let count = count_f64(chords)?;
            for step in 0..=chords {
                let angle = start + FRAC_PI_2 * count_f64(step)? / count;
                points.push(Point::new(
                    Dp::new(x + radius * angle.cos()),
                    Dp::new(y + radius * angle.sin()),
                ));
            }
        }
        Ok(points)
    }
}

pub(super) fn chords_per_quarter_turn(radius: f64, tolerance: f64) -> Result<usize, NumberError> {
    let widest = 4.0 * (tolerance / (2.0 * radius)).sqrt().clamp(0.0, 1.0).asin();
    whole_count((FRAC_PI_2 / widest).ceil())
}
