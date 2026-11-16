// SPDX-License-Identifier: GPL-3.0-only

use crate::{
    brush::Brush,
    geometry::{Color, FinitePoint, Point},
    output::finest_step,
    shape::Corners,
    units::{Dp, FiniteDp, NonNegative, NonNegativeDp, NumberError},
};

mod motion;
#[cfg(test)]
mod tests;

pub(crate) use motion::Interpolated;

#[must_use]
pub fn blur_reach(channel_step: f64) -> f64 {
    (-2.0 * (channel_step.max(finest_step()) / 2.0).ln()).sqrt()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct EffectId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SourceFilterId(pub u64);

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SourceFilter {
    pub id: SourceFilterId,
    #[cfg_attr(
        feature = "serde",
        serde(deserialize_with = "crate::encoding::sequence")
    )]
    pub parameters: Vec<f64>,
    pub outset: NonNegativeDp,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum BlendMode {
    Clear,
    Source,
    Destination,
    #[default]
    SourceOver,
    DestinationOver,
    SourceIn,
    DestinationIn,
    SourceOut,
    DestinationOut,
    SourceAtop,
    DestinationAtop,
    Xor,
    Plus,
    Modulate,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    HardLight,
    SoftLight,
    Difference,
    Exclusion,
    Multiply,
    Hue,
    Saturation,
    Color,
    Luminosity,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum BorderPlacement {
    #[default]
    Inside,
    Center,
    Outside,
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Stroke {
    pub color: Color,
    pub width: NonNegativeDp,
    pub placement: BorderPlacement,
}

impl Stroke {
    pub const fn new(color: Color, width: Dp) -> Result<Self, NumberError> {
        match NonNegativeDp::try_new(width) {
            Ok(width) => Ok(Self {
                color,
                width,
                placement: BorderPlacement::Inside,
            }),
            Err(error) => Err(error),
        }
    }

    #[must_use]
    pub const fn placement(mut self, placement: BorderPlacement) -> Self {
        self.placement = placement;
        self
    }

    pub(crate) fn paints(&self) -> bool {
        self.width.get() > Dp::ZERO && self.color.alpha() > 0.0
    }

    #[must_use]
    pub fn distances(&self) -> std::ops::Range<Dp> {
        let width = self.width.get();
        match self.placement {
            BorderPlacement::Inside => -width..Dp::ZERO,
            BorderPlacement::Center => -width / 2.0..width / 2.0,
            BorderPlacement::Outside => Dp::ZERO..width,
        }
    }

    #[must_use]
    pub fn reach(&self) -> Dp {
        self.distances().end
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Shadow {
    pub color: Color,
    pub offset: FinitePoint,
    pub blur: NonNegativeDp,
    pub spread: FiniteDp,
    pub inset: bool,
}

impl Shadow {
    pub const fn new(color: Color, blur: Dp, offset: Point) -> Result<Self, NumberError> {
        let blur = match NonNegativeDp::try_new(blur) {
            Ok(blur) => blur,
            Err(error) => return Err(error),
        };
        match FinitePoint::try_new(offset) {
            Ok(offset) => Ok(Self {
                color,
                blur,
                offset,
                spread: FiniteDp::ZERO,
                inset: false,
            }),
            Err(error) => Err(error),
        }
    }

    pub const fn spread(mut self, spread: Dp) -> Result<Self, NumberError> {
        match FiniteDp::try_new(spread) {
            Ok(spread) => {
                self.spread = spread;
                Ok(self)
            }
            Err(error) => Err(error),
        }
    }

    #[must_use]
    pub const fn inset(mut self, inset: bool) -> Self {
        self.inset = inset;
        self
    }

    #[must_use]
    pub fn reach(&self, channel_step: f64) -> Dp {
        let offset = self.offset.get();
        self.blur.get() * blur_reach(channel_step)
            + self.spread.get().abs()
            + offset.x.abs().max(offset.y.abs())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Glass {
    pub refraction: NonNegativeDp,
    pub dispersion: NonNegativeDp,
    pub rim: NonNegative,
}

impl Glass {
    #[must_use]
    pub fn reach(&self) -> Dp {
        self.refraction.get() + self.dispersion.get()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Backdrop {
    pub blur: NonNegativeDp,
    pub glass: Option<Glass>,
}

impl Backdrop {
    #[must_use]
    pub fn reach(&self, channel_step: f64) -> Dp {
        self.blur.get() * blur_reach(channel_step)
            + self.glass.map_or(Dp::ZERO, |glass| glass.reach())
    }

    pub const fn blur(sigma: Dp) -> Result<Self, NumberError> {
        match NonNegativeDp::try_new(sigma) {
            Ok(blur) => Ok(Self { blur, glass: None }),
            Err(error) => Err(error),
        }
    }

    pub const fn glass(sigma: Dp, glass: Glass) -> Result<Self, NumberError> {
        match NonNegativeDp::try_new(sigma) {
            Ok(blur) => Ok(Self {
                blur,
                glass: Some(glass),
            }),
            Err(error) => Err(error),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Material {
    pub fill: Brush,
    pub backdrop: Option<Backdrop>,
    pub shading: Option<Brush>,
    pub border: Option<Stroke>,
    pub shadows: Vec<Shadow>,
}

impl Material {
    pub fn new(fill: impl Into<Brush>) -> Self {
        Self {
            fill: fill.into(),
            backdrop: None,
            shading: None,
            border: None,
            shadows: Vec::new(),
        }
    }

    #[must_use]
    pub fn fill(mut self, fill: impl Into<Brush>) -> Self {
        self.fill = fill.into();
        self
    }

    #[must_use]
    pub fn backdrop(mut self, backdrop: Backdrop) -> Self {
        self.backdrop = Some(backdrop);
        self
    }

    #[must_use]
    pub fn shading(mut self, shading: impl Into<Brush>) -> Self {
        self.shading = Some(shading.into());
        self
    }

    #[must_use]
    pub fn border(mut self, border: Stroke) -> Self {
        self.border = Some(border);
        self
    }

    #[must_use]
    pub fn shadow(mut self, shadow: Shadow) -> Self {
        self.shadows.push(shadow);
        self
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ShapedMaterial {
    pub material: Material,
    pub corners: Corners,
}

impl ShapedMaterial {
    pub fn new(material: Material, corners: impl Into<Corners>) -> Self {
        Self {
            material,
            corners: corners.into(),
        }
    }
}
