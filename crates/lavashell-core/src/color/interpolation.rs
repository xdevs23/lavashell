// SPDX-License-Identifier: GPL-3.0-only
use super::{Color, Gamut};
use crate::animation::{Animatable, Trajectory, Vector, VectorSpace};
use crate::output::finest_step;
use crate::units::Dp;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HuePath {
    #[default]
    Shorter,
    Longer,
    Increasing,
    Decreasing,
}

impl HuePath {
    /// The signed distance from the hue `from` to the hue `to` along this path, in turns, by the hue interpolation
    /// rules of CSS Color 4.
    fn turn(self, from: f64, to: f64) -> f64 {
        let delta = to.rem_euclid(1.0) - from.rem_euclid(1.0);
        let shorter = delta - delta.round();
        if shorter.abs() <= SAME_HUE {
            return match self {
                Self::Longer => shorter + 1.0,
                _ => shorter,
            };
        }
        match self {
            Self::Shorter if delta > 0.5 => delta - 1.0,
            Self::Shorter if delta < -0.5 => delta + 1.0,
            Self::Longer if delta > 0.0 && delta < 0.5 => delta - 1.0,
            Self::Longer if delta > -0.5 && delta <= 0.0 => delta + 1.0,
            Self::Increasing if delta < 0.0 => delta + 1.0,
            Self::Decreasing if delta > 0.0 => delta - 1.0,
            _ => delta,
        }
    }

    /// The start and the end channels of a motion from `from` to `to`. The end hue differs from the start hue by
    /// the distance along this path. An end without a hue gets the hue of the other end.
    fn ends(self, from: OklchChannels, to: OklchChannels) -> (OklchChannels, OklchChannels) {
        let (from, to) = if !from.has_hue() {
            (from.with_hue(to.hue()), to)
        } else if !to.has_hue() {
            (from, to.with_hue(from.hue()))
        } else {
            (from, to)
        };
        let from = from.with_hue(from.hue().rem_euclid(1.0));
        let to = to.with_hue(from.hue() + self.turn(from.hue(), to.hue()));
        (from, to)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorInterpolation {
    Oklab,
    Oklch(HuePath),
    LinearRgb,
    Srgb,
}

impl Default for ColorInterpolation {
    fn default() -> Self {
        Self::Oklch(HuePath::default())
    }
}

impl ColorInterpolation {
    fn space(self) -> Space {
        match self {
            Self::Oklab => Space::Oklab,
            Self::Oklch(_) => Space::Oklch,
            Self::LinearRgb => Space::LinearRgb,
            Self::Srgb => Space::Srgb,
        }
    }

    fn ends(self, from: [f64; 4], to: [f64; 4]) -> ([f64; 4], [f64; 4]) {
        match self {
            Self::Oklch(path) => {
                let (from, to) = path.ends(OklchChannels(from), OklchChannels(to));
                (from.0, to.0)
            }
            _ => (from, to),
        }
    }
}

const DEGREES_PER_TURN: f64 = 360.0;

/// The chroma at or below which a color has no hue, as set by the sample code of CSS Color 4 that converts
/// `OKLab` to OKLCH.
const ACHROMATIC_CHROMA: f64 = 0.000_004;

/// The distance in turns within which two hues count as the same hue. A hue is converted through degrees as an
/// f64, which rounds in steps of 256 epsilon from 256 to 360 degrees. Two hues converted from the same hue can each
/// be one step off.
const SAME_HUE: f64 = 2.0 * 256.0 * f64::EPSILON / DEGREES_PER_TURN;

/// The alpha at or below which a color is transparent. It is the smallest positive f64 that is a normal number.
const MIN_ALPHA: f64 = f64::MIN_POSITIVE;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Space {
    Oklab,
    Oklch,
    LinearRgb,
    Srgb,
}

impl Space {
    /// The channels of `color` in this space multiplied by alpha, followed by alpha. An OKLCH hue
    /// is in turns and is not multiplied.
    fn channels(self, color: Color) -> [f64; 4] {
        let alpha = color.alpha();
        let [x, y, z] = match self {
            Self::Oklab => color.to_oklab(),
            Self::Oklch => color.to_oklch(),
            Self::LinearRgb => color.to_linear(Gamut::Srgb),
            Self::Srgb => color.to_encoded(Gamut::Srgb),
        };
        let z = match self {
            Self::Oklch => z / DEGREES_PER_TURN,
            _ => z * alpha,
        };
        [x * alpha, y * alpha, z, alpha]
    }

    fn color(self, [x, y, z, alpha]: [f64; 4]) -> Color {
        let alpha = alpha.clamp(0.0, 1.0);
        if alpha <= MIN_ALPHA {
            return Color::TRANSPARENT;
        }
        let channel = |v: f64| v / alpha;
        let color = match self {
            Self::Oklab => Color::oklab(channel(x), channel(y), channel(z)),
            Self::Oklch => {
                Color::oklch(channel(x), channel(y), z.rem_euclid(1.0) * DEGREES_PER_TURN)
            }
            Self::LinearRgb => {
                Color::linear([channel(x), channel(y), channel(z)], 1.0, Gamut::Srgb)
            }
            Self::Srgb => Color::encoded([channel(x), channel(y), channel(z)], 1.0, Gamut::Srgb),
        };
        color.with_alpha(alpha)
    }

    /// `current`, a motion in the space `from`, converted into a motion in this space. In OKLCH the hue moves to
    /// the end along the shorter path, and the hue velocity comes from the converted velocity.
    fn converted(self, from: Self, current: Trajectory<Vector<4>>) -> Trajectory<Vector<4>> {
        if self == from {
            return current;
        }
        let convert = |Vector(channels): Vector<4>| self.channels(from.color(channels));
        let (position, end) = (convert(current.position), convert(current.end));
        let velocity = self.converted_velocity(&current, &convert, position);
        let (position, end) = if self == Self::Oklch {
            let (position, end) =
                HuePath::Shorter.ends(OklchChannels(position), OklchChannels(end));
            (position.0, end.0)
        } else {
            (position, end)
        };
        Trajectory {
            position: Vector(position),
            velocity: Vector(velocity),
            end: Vector(end),
        }
    }

    /// The velocity of `current` in this space: the slope of `convert` along the velocity of `current`. The slope
    /// is a central difference over a distance of the cube root of the f64 epsilon. At that distance the truncation
    /// error and the rounding error of the difference are equal. The distance is measured in channel units, in
    /// which reference white is one.
    fn converted_velocity(
        self,
        current: &Trajectory<Vector<4>>,
        convert: &impl Fn(Vector<4>) -> [f64; 4],
        position: [f64; 4],
    ) -> [f64; 4] {
        let speed = current.velocity.max_abs();
        if speed == 0.0 {
            return [0.0; 4];
        }
        let reach = f64::EPSILON.cbrt() * current.position.max_abs().max(1.0);
        let step = reach / speed;
        let at = |time: f64| convert(current.position.add(&current.velocity.scale(time)));
        let (ahead, behind) = (at(step), at(-step));
        let span = 2.0 * step;
        let mut change: [f64; 4] = std::array::from_fn(|i| (ahead[i] - behind[i]) / span);
        if self == Self::Oklch {
            change[2] = if OklchChannels(position).has_hue() {
                HuePath::Shorter.turn(behind[2], ahead[2]) / span
            } else {
                0.0
            };
        }
        change
    }
}

/// Premultiplied OKLCH channels: lightness and chroma multiplied by alpha, the hue in turns, and
/// alpha.
#[derive(Clone, Copy)]
struct OklchChannels([f64; 4]);

impl OklchChannels {
    fn hue(self) -> f64 {
        self.0[2]
    }

    fn with_hue(mut self, hue: f64) -> Self {
        self.0[2] = hue;
        self
    }

    /// A color has no hue when its chroma is at or below the CSS Color 4 threshold, scaled by
    /// alpha so that a transparent color has no hue either.
    fn has_hue(self) -> bool {
        let [_, chroma, _, alpha] = self.0;
        chroma > ACHROMATIC_CHROMA * alpha
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColorIn {
    pub color: Color,
    pub interpolation: ColorInterpolation,
}

impl Color {
    #[must_use]
    pub const fn interpolated(self, interpolation: ColorInterpolation) -> ColorIn {
        ColorIn {
            color: self,
            interpolation,
        }
    }

    /// Mixes this color with `other` by `amount` in the space of `interpolation`, with channels premultiplied by
    /// alpha. A transparent end then adds no color of its own to the mix. Animated colors move through the same
    /// premultiplied channels.
    #[must_use]
    pub fn mix_in(self, other: Self, amount: f64, interpolation: ColorInterpolation) -> Self {
        let space = interpolation.space();
        let (from, to) = interpolation.ends(space.channels(self), space.channels(other));
        let t = amount.clamp(0.0, 1.0);
        space.color(std::array::from_fn(|i| from[i] + (to[i] - from[i]) * t))
    }

    /// Whether this color has a hue for OKLCH interpolation. A mix or a motion that starts or ends at a color
    /// without a hue uses the hue of the other color.
    #[must_use]
    pub fn has_hue(self) -> bool {
        OklchChannels(Space::Oklch.channels(self)).has_hue()
    }
}

impl From<Color> for ColorIn {
    fn from(color: Color) -> Self {
        color.interpolated(ColorInterpolation::default())
    }
}

impl From<ColorIn> for Color {
    fn from(color: ColorIn) -> Self {
        color.color
    }
}

impl Animatable for ColorIn {
    type Vector = Vector<4>;

    fn vector(&self) -> Self::Vector {
        Vector(self.interpolation.space().channels(self.color))
    }

    /// One step of a color channel at the highest bit depth of any output.
    fn visible_step(_pixel: Dp) -> f64 {
        finest_step()
    }

    fn at(&self, vector: Self::Vector) -> Self {
        let color = self.interpolation.space().color(vector.0);
        color.interpolated(self.interpolation)
    }

    /// Converts `current` into the color space of this value. When the color equals the color of `previous` and
    /// only the interpolation differs, the hue path of `current` is left unchanged.
    fn retarget(
        &self,
        previous: &Self,
        current: Trajectory<Self::Vector>,
    ) -> Trajectory<Self::Vector> {
        let space = self.interpolation.space();
        let current = space.converted(previous.interpolation.space(), current);
        if self.color == previous.color {
            return current;
        }
        let (position, end) = self.interpolation.ends(current.position.0, self.vector().0);
        Trajectory {
            position: Vector(position),
            end: Vector(end),
            ..current
        }
    }
}

impl Animatable for Color {
    type Vector = Vector<4>;

    fn vector(&self) -> Self::Vector {
        ColorIn::from(*self).vector()
    }

    fn visible_step(pixel: Dp) -> f64 {
        ColorIn::visible_step(pixel)
    }

    fn at(&self, vector: Self::Vector) -> Self {
        ColorIn::from(*self).at(vector).color
    }

    fn retarget(
        &self,
        previous: &Self,
        current: Trajectory<Self::Vector>,
    ) -> Trajectory<Self::Vector> {
        ColorIn::from(*self).retarget(&ColorIn::from(*previous), current)
    }
}

#[cfg(test)]
mod tests;
