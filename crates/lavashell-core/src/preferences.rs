// SPDX-License-Identifier: GPL-3.0-only
//! Accessibility preferences, each handed down the view tree under a separate key.
//!
//! A host stores the system values and the overrides set by the application in a
//! `PreferenceSource` and sets the resulting values at the root of each view with
//! `View::provide_preferences`.

use crate::local::Local;
use crate::units::{Dp, Sp};
use std::{fmt, sync::Arc};

#[derive(Clone)]
enum Scaling {
    Linear(f64),
    Custom(Arc<dyn Fn(Sp) -> Dp + Send + Sync>),
}

/// Converts font sizes from sp to dp before layout, shaping and drawing.
#[derive(Clone)]
pub struct TextScaling(Scaling);

impl Default for TextScaling {
    fn default() -> Self {
        Self(Scaling::Linear(1.0))
    }
}

impl TextScaling {
    #[must_use]
    pub fn linear(factor: f64) -> Option<Self> {
        (factor.is_finite() && factor > 0.0).then_some(Self(Scaling::Linear(factor)))
    }

    pub fn custom(scale: impl Fn(Sp) -> Dp + Send + Sync + 'static) -> Self {
        Self(Scaling::Custom(Arc::new(scale)))
    }

    /// `size` converted into dp. Where the scaled size is not finite or not positive, one sp becomes one dp.
    #[must_use]
    pub fn to_dp(&self, size: Sp) -> Dp {
        let scaled = match &self.0 {
            Scaling::Linear(factor) => Dp::new(size.get() * factor),
            Scaling::Custom(scale) => scale(size),
        };
        if scaled.is_finite() && scaled > Dp::ZERO {
            scaled
        } else {
            Dp::new(size.get())
        }
    }
}

impl PartialEq for TextScaling {
    fn eq(&self, other: &Self) -> bool {
        match (&self.0, &other.0) {
            (Scaling::Linear(a), Scaling::Linear(b)) => a == b,
            (Scaling::Custom(a), Scaling::Custom(b)) => Arc::ptr_eq(a, b),
            _ => false,
        }
    }
}

impl fmt::Debug for TextScaling {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Scaling::Linear(factor) => f.debug_tuple("Linear").field(&factor).finish(),
            Scaling::Custom(_) => f.write_str("Custom"),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum ContrastPreference {
    #[default]
    NoPreference,
    More,
}

pub static TEXT_SCALING: Local<TextScaling> = Local::named("text-scaling", TextScaling::default);

/// Whether the user asked for reduced motion. The motion clock of a view uses the root value of this key. A value
/// provided for a subtree changes only what code in that subtree reads.
pub static REDUCED_MOTION: Local<bool> = Local::named("reduced-motion", || false);

pub static REDUCED_TRANSPARENCY: Local<bool> = Local::named("reduced-transparency", || false);
pub static CONTRAST: Local<ContrastPreference> =
    Local::named("contrast", ContrastPreference::default);

#[derive(Clone, Debug, Default, PartialEq)]
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
