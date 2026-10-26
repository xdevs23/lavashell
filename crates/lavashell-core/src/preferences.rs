// SPDX-License-Identifier: GPL-3.0-only
//! Accessibility preferences, each handed down the view tree under a separate key.
//!
//! A host stores the system values and the overrides set by the application in a
//! `PreferenceSource` and sets the resulting values at the root of each view with
//! `View::provide_preferences`.

use crate::local::Local;
use crate::units::{Dp, Positive, Sp};
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
enum Scaling {
    Linear(f64),
    /// Font sizes in increasing order, each with the size it becomes.
    Table(
        #[cfg_attr(feature = "serde", serde(with = "crate::encoding::shared_slice"))]
        Arc<[(Sp, Dp)]>,
    ),
}

/// Converts font sizes from sp to dp before layout, shaping and drawing.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(try_from = "Scaling", into = "Scaling")
)]
pub struct TextScaling(Scaling);

/// The reason why a read text scaling is not one that `TextScaling::linear` or `TextScaling::table` makes.
#[cfg(feature = "serde")]
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum TextScalingError {
    #[error("a linear text scaling needs a finite factor above zero")]
    Factor,
    #[error("a text scaling table needs finite sizes above zero, with increasing font sizes")]
    Table,
}

#[cfg(feature = "serde")]
impl TryFrom<Scaling> for TextScaling {
    type Error = TextScalingError;

    fn try_from(scaling: Scaling) -> Result<Self, TextScalingError> {
        match scaling {
            Scaling::Linear(factor) => Self::linear(factor).ok_or(TextScalingError::Factor),
            Scaling::Table(points) => Self::checked_table(points).ok_or(TextScalingError::Table),
        }
    }
}

#[cfg(feature = "serde")]
impl From<TextScaling> for Scaling {
    fn from(scaling: TextScaling) -> Self {
        scaling.0
    }
}

impl Default for TextScaling {
    fn default() -> Self {
        Self(Scaling::Linear(1.0))
    }
}

impl TextScaling {
    #[must_use]
    pub fn linear(factor: f64) -> Option<Self> {
        positive(factor).then_some(Self(Scaling::Linear(factor)))
    }

    /// Sizes between two points of the table interpolate linearly. Below the first point the table runs from zero,
    /// and past the last point a size has the ratio of the last point, as the font scale tables of AOSP do. `None`
    /// without points, for a size that is not finite and above zero, or when the font sizes do not increase.
    #[must_use]
    pub fn table(points: impl IntoIterator<Item = (Sp, Dp)>) -> Option<Self> {
        Self::checked_table(points.into_iter().collect())
    }

    fn checked_table(points: Arc<[(Sp, Dp)]>) -> Option<Self> {
        let valid = !points.is_empty()
            && points
                .iter()
                .all(|(sp, dp)| positive(sp.get()) && positive(dp.get()))
            && points.is_sorted_by(|from, to| from.0 < to.0);
        valid.then_some(Self(Scaling::Table(points)))
    }

    /// `size` converted into dp. Where the scaled size is not finite or not positive, one sp becomes one dp.
    #[must_use]
    pub fn to_dp(&self, size: Sp) -> Dp {
        let scaled = match &self.0 {
            Scaling::Linear(factor) => Dp::new(size.get() * factor),
            Scaling::Table(points) => interpolated(points, size),
        };
        if scaled.is_finite() && scaled > Dp::ZERO {
            scaled
        } else {
            Dp::new(size.get())
        }
    }
}

fn positive(value: f64) -> bool {
    Positive::try_new(value).is_ok()
}

/// The size in dp that `points`, sorted by font size, give for `size`.
fn interpolated(points: &[(Sp, Dp)], size: Sp) -> Dp {
    let above = points.partition_point(|(sp, _)| *sp < size);
    let below = above
        .checked_sub(1)
        .and_then(|index| points.get(index))
        .copied();
    match points.get(above) {
        Some(&(next_font, next_scaled)) => {
            let (previous_font, previous_scaled) = below.unwrap_or((Sp::ZERO, Dp::ZERO));
            let span = next_font.get() - previous_font.get();
            let progress = (size.get() - previous_font.get()) / span;
            previous_scaled * (1.0 - progress) + next_scaled * progress
        }
        None => below.map_or(Dp::new(size.get()), |(last_font, last_scaled)| {
            last_scaled * (size.get() / last_font.get())
        }),
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub enum ContrastPreference {
    #[default]
    NoPreference,
    More,
}

pub static TEXT_SCALING: Local<TextScaling> = Local::new(TextScaling::default);

/// Whether the user asked for reduced motion. The motion clock of a view uses the root value of this key. A value
/// provided for a subtree changes only what code in that subtree reads.
pub static REDUCED_MOTION: Local<bool> = Local::new(|| false);

pub static REDUCED_TRANSPARENCY: Local<bool> = Local::new(|| false);
pub static CONTRAST: Local<ContrastPreference> = Local::new(ContrastPreference::default);

#[derive(Clone, Debug, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PreferenceValues {
    pub text_scaling: TextScaling,
    pub reduced_motion: bool,
    pub reduced_transparency: bool,
    pub contrast: ContrastPreference,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PreferenceOverrides {
    pub text_scaling: Option<TextScaling>,
    pub reduced_motion: Option<bool>,
    pub reduced_transparency: Option<bool>,
    pub contrast: Option<ContrastPreference>,
}

impl PreferenceOverrides {
    fn apply(&self, system: &PreferenceValues) -> PreferenceValues {
        PreferenceValues {
            text_scaling: self
                .text_scaling
                .clone()
                .unwrap_or_else(|| system.text_scaling.clone()),
            reduced_motion: self.reduced_motion.unwrap_or(system.reduced_motion),
            reduced_transparency: self
                .reduced_transparency
                .unwrap_or(system.reduced_transparency),
            contrast: self.contrast.unwrap_or(system.contrast),
        }
    }
}

/// The system values and the overrides set by the application. An override replaces the
/// system value until the overrides are set again.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PreferenceSource {
    system: PreferenceValues,
    overrides: PreferenceOverrides,
}

impl PreferenceSource {
    #[must_use]
    pub fn system(&self) -> &PreferenceValues {
        &self.system
    }

    pub fn set_system(&mut self, values: PreferenceValues) {
        self.system = values;
    }

    pub fn set_overrides(&mut self, overrides: PreferenceOverrides) {
        self.overrides = overrides;
    }

    /// The system values, with each set override in place of the matching system value.
    #[must_use]
    pub fn values(&self) -> PreferenceValues {
        self.overrides.apply(&self.system)
    }
}

#[cfg(test)]
mod tests;
