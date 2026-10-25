// SPDX-License-Identifier: GPL-3.0-only
//! The animation of materials, strokes, shadows, glass and backdrops. When a motion produces a number that is not
//! finite, `with_parts` uses the matching number of the effect on which it is called.
use super::{Backdrop, Glass, Material, Shadow, ShapedMaterial, Stroke};
use crate::animation::{Composite, Fading, composite_fields};
use crate::color::{ColorIn, ColorInterpolation};
use crate::geometry::{Color, FinitePoint, Point};
use crate::scene::Brush;
use crate::shape::Corners;
use crate::units::{Dp, FiniteDp, NonNegative, NonNegativeDp};

mod brush;
#[cfg(test)]
mod tests;

/// A value whose colors animate in the color space chosen by `interpolation`.
#[derive(Clone, Debug, PartialEq)]
pub struct Interpolated<T> {
    value: T,
    interpolation: ColorInterpolation,
}

impl<T> Interpolated<T> {
    pub(crate) fn new(value: T, interpolation: ColorInterpolation) -> Self {
        Self {
            value,
            interpolation,
        }
    }

    pub(crate) fn value(&self) -> &T {
        &self.value
    }

    pub(crate) fn into_value(self) -> T {
        self.value
    }

    fn in_space(&self, color: Color) -> ColorIn {
        color.interpolated(self.interpolation)
    }

    fn with_value<U>(&self, value: U) -> Interpolated<U> {
        Interpolated::new(value, self.interpolation)
    }
}

fn transparent(color: Color) -> Color {
    color.with_alpha(0.0)
}

impl Composite for Material {
    type Parts = Interpolated<Material>;

    fn parts(&self) -> Interpolated<Material> {
        Interpolated::new(self.clone(), ColorInterpolation::default())
    }

    fn with_parts(&self, parts: Interpolated<Material>) -> Self {
        parts.value
    }
}

composite_fields! {
    pub struct StrokeParts {
        color: ColorIn,
        width: Dp,
    }
}

impl Composite for Interpolated<Stroke> {
    type Parts = StrokeParts;

    fn parts(&self) -> StrokeParts {
        StrokeParts {
            color: self.in_space(self.value.color),
            width: self.value.width.get(),
        }
    }

    fn with_parts(&self, parts: StrokeParts) -> Self {
        self.with_value(Stroke {
            color: parts.color.color,
            width: NonNegativeDp::saturating(parts.width).unwrap_or(self.value.width),
            ..self.value
        })
    }
}

impl Fading for Interpolated<Stroke> {
    fn faded(&self) -> Self {
        self.with_parts(StrokeParts {
            color: self.in_space(transparent(self.value.color)),
            width: Dp::ZERO,
        })
    }
}

composite_fields! {
    pub struct ShadowParts {
        color: ColorIn,
        offset: Point,
        blur: Dp,
        spread: Dp,
    }
}

impl Composite for Interpolated<Shadow> {
    type Parts = ShadowParts;

    fn parts(&self) -> ShadowParts {
        let shadow = &self.value;
        ShadowParts {
            color: self.in_space(shadow.color),
            offset: shadow.offset.get(),
            blur: shadow.blur.get(),
            spread: shadow.spread.get(),
        }
    }

    fn with_parts(&self, parts: ShadowParts) -> Self {
        let shadow = &self.value;
        self.with_value(Shadow {
            color: parts.color.color,
            offset: FinitePoint::try_new(parts.offset).unwrap_or(shadow.offset),
            blur: NonNegativeDp::saturating(parts.blur).unwrap_or(shadow.blur),
            spread: FiniteDp::try_new(parts.spread).unwrap_or(shadow.spread),
            inset: shadow.inset,
        })
    }
}

impl Fading for Interpolated<Shadow> {
    fn faded(&self) -> Self {
        self.with_value(Shadow {
            color: transparent(self.value.color),
            ..self.value
        })
    }
}

composite_fields! {
    pub struct GlassParts {
        refraction: Dp,
        dispersion: Dp,
        rim: f64,
    }
}

impl Composite for Glass {
    type Parts = GlassParts;

    fn parts(&self) -> GlassParts {
        GlassParts {
            refraction: self.refraction.get(),
            dispersion: self.dispersion.get(),
            rim: self.rim.get(),
        }
    }

    fn with_parts(&self, parts: GlassParts) -> Self {
        Self {
            refraction: NonNegativeDp::saturating(parts.refraction).unwrap_or(self.refraction),
            dispersion: NonNegativeDp::saturating(parts.dispersion).unwrap_or(self.dispersion),
            rim: NonNegative::saturating(parts.rim).unwrap_or(self.rim),
        }
    }
}

impl Fading for Glass {
    fn faded(&self) -> Self {
        self.with_parts(GlassParts {
            refraction: Dp::ZERO,
            dispersion: Dp::ZERO,
            rim: 0.0,
        })
    }
}

impl Composite for Backdrop {
    type Parts = (Dp, Option<Glass>);

    fn parts(&self) -> (Dp, Option<Glass>) {
        (self.blur.get(), self.glass)
    }

    fn with_parts(&self, (blur, glass): (Dp, Option<Glass>)) -> Self {
        Self {
            blur: NonNegativeDp::saturating(blur).unwrap_or(self.blur),
            glass,
        }
    }
}

impl Fading for Backdrop {
    fn faded(&self) -> Self {
        self.with_parts((Dp::ZERO, self.glass.as_ref().map(Fading::faded)))
    }
}

composite_fields! {
    pub struct MaterialParts {
        fill: Interpolated<Brush>,
        shading: Option<Interpolated<Brush>>,
        backdrop: Option<Backdrop>,
        border: Option<Interpolated<Stroke>>,
        outer: Vec<Interpolated<Shadow>>,
        inset: Vec<Interpolated<Shadow>>,
    }
}

/// The outer and the inset shadows animate as two separate lists. A motion then matches an outer shadow only with an
/// outer shadow and an inset shadow only with an inset shadow.
impl Composite for Interpolated<Material> {
    type Parts = MaterialParts;

    fn parts(&self) -> MaterialParts {
        let material = &self.value;
        let shadows = |inset: bool| {
            let matching = material.shadows.iter().filter(move |s| s.inset == inset);
            matching.map(|shadow| self.with_value(*shadow)).collect()
        };
        MaterialParts {
            fill: self.with_value(material.fill.clone()),
            shading: material.shading.clone().map(|brush| self.with_value(brush)),
            backdrop: material.backdrop,
            border: material.border.map(|stroke| self.with_value(stroke)),
            outer: shadows(false),
            inset: shadows(true),
        }
    }

    /// Builds the material from `parts`. The animated shadows take the places of the shadows of this material in
    /// their order. The shadows that `parts` has beyond those follow at the end.
    fn with_parts(&self, parts: MaterialParts) -> Self {
        let (mut outer, mut inset) = (parts.outer.into_iter(), parts.inset.into_iter());
        let mut shadows: Vec<Shadow> = (self.value.shadows.iter())
            .filter_map(|s| if s.inset { inset.next() } else { outer.next() })
            .map(Interpolated::into_value)
            .collect();
        shadows.extend(outer.chain(inset).map(Interpolated::into_value));
        self.with_value(Material {
            fill: parts.fill.value,
            backdrop: parts.backdrop,
            shading: parts.shading.map(Interpolated::into_value),
            border: parts.border.map(Interpolated::into_value),
            shadows,
        })
    }
}

impl Fading for Interpolated<Material> {
    fn faded(&self) -> Self {
        let parts = self.parts();
        let faded =
            |shadows: Vec<Interpolated<Shadow>>| shadows.iter().map(Fading::faded).collect();
        self.with_parts(MaterialParts {
            fill: parts.fill.faded(),
            shading: parts.shading.as_ref().map(Fading::faded),
            backdrop: parts.backdrop.as_ref().map(Fading::faded),
            border: parts.border.as_ref().map(Fading::faded),
            outer: faded(parts.outer),
            inset: faded(parts.inset),
        })
    }
}

impl Composite for Interpolated<ShapedMaterial> {
    type Parts = (Interpolated<Material>, Corners);

    fn parts(&self) -> (Interpolated<Material>, Corners) {
        (
            self.with_value(self.value.material.clone()),
            self.value.corners,
        )
    }

    fn with_parts(&self, (material, corners): (Interpolated<Material>, Corners)) -> Self {
        self.with_value(ShapedMaterial::new(material.value, corners))
    }
}

impl Fading for Interpolated<ShapedMaterial> {
    fn faded(&self) -> Self {
        let (material, corners) = self.parts();
        self.with_parts((material.faded(), corners))
    }
}
