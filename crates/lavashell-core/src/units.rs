// SPDX-License-Identifier: GPL-3.0-only
//! Lengths in density-independent pixels (dp), physical pixels (px) and
//! scale-independent pixels (sp), and letter spacing in em. A `Density` converts
//! between dp and px, and a `TextScaling` converts sp to dp.

mod checked;
mod positive;

pub use checked::{FiniteDp, Fraction, NonNegative, NonNegativeDp, NumberError};
pub use positive::{Positive, PositiveDp};

#[cfg(test)]
mod tests;

use std::cmp::Ordering;
use std::iter::Sum;
use std::ops::{
    Add, AddAssign, Div, DivAssign, Mul, MulAssign, Neg, RangeInclusive, Sub, SubAssign,
};

/// ```
/// # use lavashell_core::units::Dp;
/// assert_eq!(Dp::new(2.0) * 3.0 + Dp::new(1.0), Dp::new(7.0));
/// ```
/// These do not compile:
/// ```compile_fail
/// # use lavashell_core::units::Dp;
/// let area = Dp::new(2.0) * Dp::new(3.0);
/// ```
/// ```compile_fail
/// # use lavashell_core::units::Dp;
/// let length = Dp::new(2.0) + 1.0;
/// ```
/// ```compile_fail
/// # use lavashell_core::units::Dp;
/// let length: Dp = 1.0.into();
/// ```
/// ```compile_fail
/// # use lavashell_core::units::{Dp, Px};
/// let length = Dp::new(2.0) + Px::new(1.0);
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct Dp(f64);

#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct Px(f64);

/// A font size in scale-independent pixels, before the text size setting of the user is applied.
/// These do not compile:
/// ```compile_fail
/// # use lavashell_core::units::{DpExt, SpExt};
/// let size = 14.sp() + 2.dp();
/// ```
/// ```compile_fail
/// # use lavashell_core::units::{Dp, SpExt};
/// let size: Dp = 14.sp().into();
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct Sp(f64);

/// A fraction of the font size, so letter spacing grows with the text.
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct Em(f64);

/// Percent of the viewport width.
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct Vw(f64);

/// Percent of the viewport height.
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct Vh(f64);

/// Percent of a box: of its width on the horizontal axis, of its height on the vertical axis.
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct Pct(f64);

macro_rules! unit {
    ($unit:ident, $literal:ident, $method:ident) => {
        impl $unit {
            pub const fn new(value: f64) -> Self {
                Self(value)
            }
            pub const fn get(self) -> f64 {
                self.0
            }
        }
        #[doc = concat!("Writes `16.", stringify!($method), "()` or `0.5.", stringify!($method), "()`.")]
        pub trait $literal {
            fn $method(self) -> $unit;
        }
        // With one integer type and one float type, unsuffixed literals are unambiguous.
        impl $literal for i32 {
            fn $method(self) -> $unit {
                $unit(f64::from(self))
            }
        }
        impl $literal for f64 {
            fn $method(self) -> $unit {
                $unit(self)
            }
        }
    };
}

unit!(Dp, DpExt, dp);

unit!(Px, PxExt, px);

unit!(Sp, SpExt, sp);

unit!(Em, EmExt, em);

unit!(Vw, VwExt, vw);

unit!(Vh, VhExt, vh);

unit!(Pct, PctExt, pct);

impl Add for Dp {
    type Output = Self;

    fn add(self, rhs: Self) -> Self {
        Self(self.0 + rhs.0)
    }
}

impl Sub for Dp {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self {
        Self(self.0 - rhs.0)
    }
}

impl Mul<f64> for Dp {
    type Output = Self;

    fn mul(self, factor: f64) -> Self {
        Self(self.0 * factor)
    }
}

impl Mul<Dp> for f64 {
    type Output = Dp;

    fn mul(self, length: Dp) -> Dp {
        length * self
    }
}

impl Div<f64> for Dp {
    type Output = Self;

    fn div(self, divisor: f64) -> Self {
        Self(self.0 / divisor)
    }
}

impl Div for Dp {
    type Output = f64;

    fn div(self, rhs: Self) -> f64 {
        self.0 / rhs.0
    }
}

macro_rules! negation {
    ($($unit:ident),*) => {$(
        impl Neg for $unit {
            type Output = Self;
            fn neg(self) -> Self {
                Self(-self.0)
            }
        }
    )*};
}

negation!(Dp, Vw, Vh, Pct);

impl Dp {
    pub const ZERO: Self = Self(0.0);
    pub const MAX: Self = Self(f64::MAX);
    pub const INFINITY: Self = Self(f64::INFINITY);

    #[must_use]
    pub fn min(self, other: Self) -> Self {
        Self(self.0.min(other.0))
    }

    #[must_use]
    pub fn max(self, other: Self) -> Self {
        Self(self.0.max(other.0))
    }

    #[must_use]
    pub fn clamp(self, min: Self, max: Self) -> Self {
        Self(self.0.clamp(min.0, max.0))
    }

    #[must_use]
    pub fn abs(self) -> Self {
        Self(self.0.abs())
    }

    #[must_use]
    pub fn floor(self) -> Self {
        Self(self.0.floor())
    }

    #[must_use]
    pub fn ceil(self) -> Self {
        Self(self.0.ceil())
    }

    #[must_use]
    pub fn round(self) -> Self {
        Self(self.0.round())
    }

    #[must_use]
    pub fn hypot(self, other: Self) -> Self {
        Self(self.0.hypot(other.0))
    }

    #[must_use]
    pub fn is_finite(self) -> bool {
        self.0.is_finite()
    }

    #[must_use]
    pub fn total_cmp(&self, other: &Self) -> Ordering {
        self.0.total_cmp(&other.0)
    }
}

impl AddAssign for Dp {
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
    }
}

impl SubAssign for Dp {
    fn sub_assign(&mut self, rhs: Self) {
        *self = *self - rhs;
    }
}

impl MulAssign<f64> for Dp {
    fn mul_assign(&mut self, factor: f64) {
        *self = *self * factor;
    }
}

impl DivAssign<f64> for Dp {
    fn div_assign(&mut self, divisor: f64) {
        *self = *self / divisor;
    }
}

impl Sum for Dp {
    fn sum<I: Iterator<Item = Self>>(lengths: I) -> Self {
        Self(lengths.map(Self::get).sum())
    }
}

impl<'a> Sum<&'a Dp> for Dp {
    fn sum<I: Iterator<Item = &'a Self>>(lengths: I) -> Self {
        lengths.copied().sum()
    }
}

impl Add for Sp {
    type Output = Self;

    fn add(self, rhs: Self) -> Self {
        Self(self.0 + rhs.0)
    }
}

impl Mul<f64> for Sp {
    type Output = Self;

    fn mul(self, factor: f64) -> Self {
        Self(self.0 * factor)
    }
}

impl Sp {
    pub const ZERO: Self = Self(0.0);

    #[must_use]
    pub fn min(self, other: Self) -> Self {
        Self(self.0.min(other.0))
    }

    #[must_use]
    pub fn max(self, other: Self) -> Self {
        Self(self.0.max(other.0))
    }

    #[must_use]
    pub fn floor(self) -> Self {
        Self(self.0.floor())
    }
}

impl Em {
    pub const ZERO: Self = Self(0.0);

    #[must_use]
    pub fn to_dp(self, font_size: Dp) -> Dp {
        font_size * self.0
    }
}

/// The dpi at which one dp is one physical pixel, by the definition of the dp unit.
const BASELINE_DPI: f64 = 160.0;

/// Physical pixels per dp, always finite and positive: the dpi of a display divided by 160,
/// or the scale set by a host for the surface. `Density::ONE` makes one dp one pixel:
/// ```
/// # use lavashell_core::units::{Density, DpExt, PxExt};
/// assert_eq!(Density::ONE.to_px(1.dp()), 1.px());
/// ```
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct Density(f64);

impl Density {
    pub const ONE: Self = Self(1.0);

    #[must_use]
    pub fn from_scale(scale: f64) -> Option<Self> {
        (scale.is_finite() && scale > 0.0).then_some(Self(scale))
    }

    /// # Errors
    ///
    /// Fails when `dpi` is not finite or not above zero, or when the density computed from it is not.
    pub const fn from_dpi(dpi: f64) -> Result<Self, NumberError> {
        Self::from_ratio(dpi, BASELINE_DPI)
    }

    /// Returns the density at which `dp` dp cover `pixels` physical pixels.
    ///
    /// # Errors
    ///
    /// Fails when `pixels` or `dp` is not finite or not above zero, or when their quotient is not.
    pub const fn from_ratio(pixels: f64, dp: f64) -> Result<Self, NumberError> {
        match (Positive::try_new(pixels), Positive::try_new(dp)) {
            (Ok(pixels), Ok(dp)) => match Positive::try_new(pixels.get() / dp.get()) {
                Ok(density) => Ok(Self(density.get())),
                Err(error) => Err(error),
            },
            (Err(error), _) | (_, Err(error)) => Err(error),
        }
    }

    #[must_use]
    pub const fn get(self) -> f64 {
        self.0
    }

    #[must_use]
    pub fn to_px(self, length: Dp) -> Px {
        length * self
    }

    #[must_use]
    pub fn to_dp(self, length: Px) -> Dp {
        length / self
    }
}

impl Mul<Density> for Dp {
    type Output = Px;

    fn mul(self, density: Density) -> Px {
        Px(self.0 * density.0)
    }
}

impl Div<Density> for Px {
    type Output = Dp;

    fn div(self, density: Density) -> Dp {
        Dp(self.0 / density.0)
    }
}

/// A scale about the center of a node, within `ScaleFactor::RANGE`.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub struct ScaleFactor(f64);

impl ScaleFactor {
    pub const ONE: Self = Self(1.0);

    /// From 2 to the power of minus a third of the largest `f64` exponent up to 2 to the power of that third.
    /// Three factors from this range multiply to a finite `f64`. A length of up to the largest scale under two
    /// nested scales is therefore still finite.
    pub const RANGE: RangeInclusive<f64> = {
        let mut largest = 1.0_f64;
        let mut doublings = 0;
        while doublings < f64::MAX_EXP / 3 {
            largest *= 2.0;
            doublings += 1;
        }
        1.0 / largest..=largest
    };

    /// # Errors
    ///
    /// Fails when `scale` is not finite or lies outside `ScaleFactor::RANGE`.
    pub const fn try_new(scale: f64) -> Result<Self, NumberError> {
        if !scale.is_finite() {
            Err(NumberError::NotFinite)
        } else if scale >= *Self::RANGE.start() && scale <= *Self::RANGE.end() {
            Ok(Self(scale))
        } else {
            Err(NumberError::OutOfRange)
        }
    }

    /// The nearest scale within `ScaleFactor::RANGE`.
    ///
    /// # Errors
    ///
    /// Fails when `scale` is not a number.
    pub fn saturating(scale: f64) -> Result<Self, NumberError> {
        Self::try_new(scale.clamp(*Self::RANGE.start(), *Self::RANGE.end()))
    }

    #[must_use]
    pub const fn get(self) -> f64 {
        self.0
    }
}
