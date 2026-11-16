// SPDX-License-Identifier: GPL-3.0-only
use super::{Color, DEGREES_PER_TURN, Gamut};
use crate::animation::{Animatable, Trajectory, Vector, VectorSpace};
use crate::output::finest_step;
use crate::units::Dp;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum HuePath {
    #[default]
    Shorter,
    Longer,
    Increasing,
    Decreasing,
}

impl HuePath {
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
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
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

const ACHROMATIC_CHROMA: f64 = 0.000_004;

const SAME_HUE: f64 = 2.0 * 256.0 * f64::EPSILON / DEGREES_PER_TURN;

const MIN_ALPHA: f64 = f64::MIN_POSITIVE;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Space {
    Oklab,
    Oklch,
    LinearRgb,
    Srgb,
}

impl Space {
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

#[derive(Clone, Copy)]
struct OklchChannels([f64; 4]);

impl OklchChannels {
    fn chroma(self) -> f64 {
        self.0[1]
    }

    fn hue(self) -> f64 {
        self.0[2]
    }

    fn alpha(self) -> f64 {
        self.0[3]
    }

    fn with_hue(mut self, hue: f64) -> Self {
        self.0[2] = hue;
        self
    }

    fn has_hue(self) -> bool {
        self.chroma() > ACHROMATIC_CHROMA * self.alpha()
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

    #[must_use]
    pub fn mix_in(self, other: Self, amount: f64, interpolation: ColorInterpolation) -> Self {
        let space = interpolation.space();
        let (from, to) = interpolation.ends(space.channels(self), space.channels(other));
        let t = amount.clamp(0.0, 1.0);
        space.color(std::array::from_fn(|i| from[i] + (to[i] - from[i]) * t))
    }

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

    fn visible_step(_pixel: Dp) -> f64 {
        finest_step()
    }

    fn at(&self, vector: Self::Vector) -> Self {
        let color = self.interpolation.space().color(vector.0);
        color.interpolated(self.interpolation)
    }

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
