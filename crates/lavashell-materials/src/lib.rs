// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]

use lavashell_core::prelude::*;
use lavashell_enum_values::enum_with_values;

const WARM_NEUTRAL_HUE: f64 = 80.0;

const REFRACTIVE_GLASS_LENS: Glass = Glass {
    refraction: constant!(NonNegativeDp::try_new(Dp::new(12.0))),
    dispersion: constant!(NonNegativeDp::try_new(Dp::new(0.7))),
    rim: constant!(NonNegative::try_new(0.14)),
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TreatmentParameters {
    pub blur: Option<NonNegativeDp>,
    pub opacity: f64,
    pub refraction: Option<NonNegativeDp>,
}

impl TreatmentParameters {
    fn overriding(self, treatment_defaults: Self) -> Self {
        Self {
            blur: treatment_defaults
                .blur
                .map(|default| self.blur.unwrap_or(default)),
            opacity: self.opacity,
            refraction: treatment_defaults
                .refraction
                .map(|default| self.refraction.unwrap_or(default)),
        }
    }

    fn backdrop(self) -> Option<Backdrop> {
        let blur = self.blur?;
        let glass = self.refraction.map(|refraction| Glass {
            refraction,
            ..REFRACTIVE_GLASS_LENS
        });
        Some(Backdrop { blur, glass })
    }
}

enum_with_values! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum SurfaceTreatment {
        RefractiveGlass,
        FrostedGlass,
        Translucent,
        Paper,
        Ceramic,
        BrushedMetal,
    }

    pub const ALL;
}

impl SurfaceTreatment {
    #[must_use]
    pub const fn parameters(self) -> TreatmentParameters {
        let (blur, opacity, refraction) = match self {
            Self::RefractiveGlass => (
                Some(constant!(NonNegativeDp::try_new(Dp::new(3.0)))),
                0.09,
                Some(REFRACTIVE_GLASS_LENS.refraction),
            ),
            Self::FrostedGlass => (
                Some(constant!(NonNegativeDp::try_new(Dp::new(18.0)))),
                0.18,
                None,
            ),
            Self::Translucent => (None, 0.4, None),
            Self::Paper | Self::Ceramic | Self::BrushedMetal => (None, 1.0, None),
        };
        TreatmentParameters {
            blur,
            opacity,
            refraction,
        }
    }

    #[must_use]
    pub fn material(self, tint: Color, size: Size) -> Material {
        self.material_with(tint, size, self.parameters())
    }

    #[must_use]
    pub fn material_with(
        self,
        tint: Color,
        size: Size,
        parameters: TreatmentParameters,
    ) -> Material {
        let base = Material {
            backdrop: parameters.overriding(self.parameters()).backdrop(),
            ..shadowed_surface(tint)
        };
        let opacity = parameters.opacity;
        match self {
            Self::RefractiveGlass => refractive_glass(base, tint, opacity, size),
            Self::FrostedGlass => frosted_glass(base, tint, opacity, size),
            Self::Translucent => base.fill(tint.with_alpha(opacity)),
            Self::Paper => paper(base, opacity),
            Self::Ceramic => ceramic(base, tint, opacity, size),
            Self::BrushedMetal => brushed_metal(base, opacity, size),
        }
    }
}

fn shadowed_surface(tint: Color) -> Material {
    Material::new(tint)
        .shadow(constant!(Shadow::new(
            Color::oklch(0.12, 0.015, WARM_NEUTRAL_HUE).with_alpha(0.24),
            Dp::new(18.0),
            Point::new(Dp::ZERO, Dp::new(18.0)),
        )))
        .border(constant!(Stroke::new(
            Color::WHITE.with_alpha(0.7),
            Dp::new(1.0),
        )))
}

fn diagonal_shading(size: Size, stops: &[ColorStop]) -> Gradient {
    let kind = GradientKind::Linear {
        from: Point::new(0.dp(), 0.dp()),
        to: Point::new(size.width, size.height),
    };
    Gradient::new(kind, stops)
}

fn refractive_glass(base: Material, tint: Color, opacity: f64, size: Size) -> Material {
    let white = Color::WHITE;
    base.fill(tint.with_alpha(opacity))
        .shading(diagonal_shading(
            size,
            &[
                ColorStop::new(0.0, white.with_alpha(0.19)),
                ColorStop::new(0.5, white.with_alpha(0.01)),
                ColorStop::new(1.0, white.with_alpha(0.12)),
            ],
        ))
}

fn frosted_glass(base: Material, tint: Color, opacity: f64, size: Size) -> Material {
    let white = Color::WHITE;
    base.fill(tint.with_alpha(opacity))
        .shading(diagonal_shading(
            size,
            &[
                ColorStop::new(0.0, white.with_alpha(0.24)),
                ColorStop::new(1.0, white.with_alpha(0.04)),
            ],
        ))
}

fn paper(base: Material, opacity: f64) -> Material {
    base.fill(Color::oklch(0.97, 0.016, WARM_NEUTRAL_HUE).with_alpha(opacity))
        .border(constant!(Stroke::new(
            Color::oklch(0.85, 0.025, WARM_NEUTRAL_HUE),
            Dp::new(1.0),
        )))
}

fn ceramic(base: Material, tint: Color, opacity: f64, size: Size) -> Material {
    let white = Color::WHITE;
    base.fill(tint.with_alpha(opacity))
        .shading(diagonal_shading(
            size,
            &[
                ColorStop::new(0.0, white.with_alpha(0.8)),
                ColorStop::new(0.5, white.with_alpha(0.32)),
                ColorStop::new(
                    1.0,
                    Color::oklch(0.25, 0.025, WARM_NEUTRAL_HUE).with_alpha(0.18),
                ),
            ],
        ))
}

fn brushed_metal(base: Material, opacity: f64, size: Size) -> Material {
    let white = Color::WHITE;
    base.fill(Color::oklch(0.78, 0.014, WARM_NEUTRAL_HUE).with_alpha(opacity))
        .shading(Gradient::new(
            GradientKind::Linear {
                from: Point::new(0.dp(), 0.dp()),
                to: Point::new(size.width, 0.dp()),
            },
            [
                ColorStop::new(0.0, white.with_alpha(0.15)),
                ColorStop::new(1.0 / 3.0, white.with_alpha(0.65)),
                ColorStop::new(
                    2.0 / 3.0,
                    Color::oklch(0.25, 0.01, WARM_NEUTRAL_HUE).with_alpha(0.2),
                ),
                ColorStop::new(1.0, white.with_alpha(0.5)),
            ],
        ))
        .shadow(
            constant!(Shadow::new(
                Color::oklch(0.15, 0.01, WARM_NEUTRAL_HUE).with_alpha(0.2),
                Dp::new(3.0),
                Point::new(Dp::ZERO, Dp::new(2.0)),
            ))
            .inset(true),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIZE: Size = Size::new(Dp::new(100.0), Dp::new(50.0));

    const TINT: Color = Color::oklch(0.93, 0.035, WARM_NEUTRAL_HUE);

    fn changed() -> TreatmentParameters {
        TreatmentParameters {
            blur: Some(constant!(NonNegativeDp::try_new(Dp::new(7.0)))),
            opacity: 0.5,
            refraction: Some(constant!(NonNegativeDp::try_new(Dp::new(5.0)))),
        }
    }

    #[test]
    fn a_treatment_builds_the_values_of_its_parameters() {
        for treatment in SurfaceTreatment::ALL {
            let parameters = treatment.parameters();
            let backdrop = treatment.material(TINT, SIZE).backdrop;
            assert_eq!(backdrop.map(|backdrop| backdrop.blur), parameters.blur);
            assert_eq!(
                backdrop.and_then(|backdrop| backdrop.glass.map(|glass| glass.refraction)),
                parameters.refraction
            );
        }
    }

    #[test]
    fn changed_parameters_are_built_where_the_treatment_has_them() {
        let glass = SurfaceTreatment::RefractiveGlass.material_with(TINT, SIZE, changed());
        assert_eq!(glass.fill, Brush::from(TINT.with_alpha(0.5)));
        assert_eq!(
            glass.backdrop,
            Some(constant!(Backdrop::glass(
                Dp::new(7.0),
                Glass {
                    refraction: constant!(NonNegativeDp::try_new(Dp::new(5.0))),
                    ..REFRACTIVE_GLASS_LENS
                }
            )))
        );
        let frosted = SurfaceTreatment::FrostedGlass.material_with(TINT, SIZE, changed());
        assert_eq!(
            frosted.backdrop,
            Some(constant!(Backdrop::blur(Dp::new(7.0))))
        );
    }

    #[test]
    fn a_parameter_that_the_treatment_lacks_is_ignored_and_one_that_it_has_is_kept() {
        let paper = SurfaceTreatment::Paper.material_with(TINT, SIZE, changed());
        assert_eq!(paper.backdrop, None);
        let lacking = TreatmentParameters {
            blur: None,
            refraction: None,
            ..changed()
        };
        let glass = SurfaceTreatment::RefractiveGlass.material_with(TINT, SIZE, lacking);
        assert_eq!(
            glass.backdrop,
            SurfaceTreatment::RefractiveGlass
                .material(TINT, SIZE)
                .backdrop
        );
    }
}
