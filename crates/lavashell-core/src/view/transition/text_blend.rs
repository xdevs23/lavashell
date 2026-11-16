// SPDX-License-Identifier: GPL-3.0-only
use crate::animation::{Composite, Fading, composite_fields};
use crate::color::{ColorIn, ColorInterpolation};
use crate::scene::{FontWeight, TextStyle};
use crate::units::{Dp, Em};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::view) struct TextBlend {
    color: ColorIn,
    weight: FontWeight,
    letter_spacing: Em,
}

composite_fields! {
    pub(in crate::view) struct TextBlendParts {
        color: ColorIn,
        weight: f64,
        letter_spacing: f64,
    }
}

impl TextBlend {
    pub fn of(style: &TextStyle<Dp>, interpolation: ColorInterpolation) -> Self {
        Self {
            color: style.color.interpolated(interpolation),
            weight: style.weight,
            letter_spacing: style.letter_spacing,
        }
    }

    pub fn applied_to(self, style: &TextStyle<Dp>) -> TextStyle<Dp> {
        TextStyle {
            color: self.color.color,
            weight: self.weight,
            letter_spacing: self.letter_spacing,
            ..style.clone()
        }
    }
}

impl Composite for TextBlend {
    type Parts = TextBlendParts;

    fn parts(&self) -> TextBlendParts {
        TextBlendParts {
            color: self.color,
            weight: self.weight.get(),
            letter_spacing: self.letter_spacing.get(),
        }
    }

    fn with_parts(&self, parts: TextBlendParts) -> Self {
        Self {
            color: parts.color,
            weight: FontWeight::saturating(parts.weight).unwrap_or(self.weight),
            letter_spacing: Em::new(parts.letter_spacing),
        }
    }
}

impl Fading for TextBlend {
    fn faded(&self) -> Self {
        Self {
            color: ColorIn {
                color: self.color.color.with_alpha(0.0),
                ..self.color
            },
            ..*self
        }
    }
}

#[cfg(test)]
mod tests;
