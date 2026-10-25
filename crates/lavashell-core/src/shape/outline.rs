// SPDX-License-Identifier: GPL-3.0-only
use super::{Corners, RoundedRect};
use crate::geometry::Point;
use crate::numbers::{count_f64, whole_count};
use crate::units::{Dp, NumberError};
use std::f64::consts::{FRAC_PI_2, PI};

impl RoundedRect {
    /// The rounded rectangle as a closed polygon, clockwise on screen. On each corner arc, every chord of the polygon
    /// lies at most half of `device_pixel` inside the arc, because placing a point on the pixel grid moves it that
    /// far anyway. Where rounding the coordinates to `f64` moves them further, the chords lie within that rounding
    /// instead. A corner with a radius of zero is a single point.
    ///
    /// # Errors
    ///
    /// Fails when a radius is not finite, because the number of chords for that corner is then not finite either.
    pub fn outline(&self, device_pixel: Dp) -> Result<Vec<Point>, NumberError> {
        let (left, top) = (self.rect.x, self.rect.y);
        let (right, bottom) = (left + self.rect.width, top + self.rect.height);
        let Corners {
            top_left,
            top_right,
            bottom_right,
            bottom_left,
        } = self.fitted_corners();
        // The center, radius and start angle of each arc, with y pointing down.
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
            // Half the gap between 1 and the next `f64` is the largest relative error of rounding to an `f64`.
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

/// The smallest number of chords into which a quarter turn of an arc with `radius` can be cut while the middle of
/// every chord lies at most `tolerance` inside the arc. The middle of a chord across the angle `a` lies
/// `r (1 - cos(a / 2))`, which is `2 r sin(a / 4)^2`, inside the arc.
pub(super) fn chords_per_quarter_turn(radius: f64, tolerance: f64) -> Result<usize, NumberError> {
    let widest = 4.0 * (tolerance / (2.0 * radius)).sqrt().clamp(0.0, 1.0).asin();
    whole_count((FRAC_PI_2 / widest).ceil())
}
