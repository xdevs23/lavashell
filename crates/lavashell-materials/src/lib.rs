// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]
//! Ready-made materials for surfaces, such as liquid glass, paper and brushed metal. They are in a crate
//! separate from lavashell-components so that an app can leave them out. Each `Material` returned here is a
//! plain value that the caller can change.

use lavashell_core::prelude::*;

/// The lens of the liquid glass treatment: how far it bends and splits the light from behind the surface and how
/// bright its rim is. Each value is a choice of the visual design.
const LIQUID_GLASS_LENS: Glass = Glass {
    refraction: constant!(NonNegativeDp::try_new(Dp::new(12.0))),
    dispersion: constant!(NonNegativeDp::try_new(Dp::new(0.7))),
    rim: constant!(NonNegative::try_new(0.14)),
};

/// The values of a surface treatment that a caller can change and pass to `SurfaceTreatment::material_with`.
/// `blur` and `refraction` are `None` for a treatment without a blur or a refraction.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TreatmentParameters {
    /// The sigma of the blur applied to the content behind the surface.
    pub blur: Option<NonNegativeDp>,
    /// The opacity of the fill of the surface.
    pub opacity: f64,
    /// The largest distance by which the lens of the glass shifts the content behind it.
    pub refraction: Option<NonNegativeDp>,
}

impl TreatmentParameters {
    /// These parameters limited to the effects of the treatment whose default values are `own`. A
    /// blur or a refraction missing from `own` is dropped. One that `own` has but these parameters
    /// lack takes its value from `own`.
    fn limited_to(self, own: Self) -> Self {
        Self {
            blur: own.blur.map(|own| self.blur.unwrap_or(own)),
            opacity: self.opacity,
            refraction: own.refraction.map(|own| self.refraction.unwrap_or(own)),
        }
    }

    /// The backdrop for these parameters: the blur with the liquid glass lens when there is a
    /// refraction, only the blur when there is none, or no backdrop when there is no blur.
    fn backdrop(self) -> Option<Backdrop> {
        let blur = self.blur?;
        let glass = self.refraction.map(|refraction| Glass {
            refraction,
            ..LIQUID_GLASS_LENS
        });
        Some(Backdrop { blur, glass })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfaceTreatment {
    LiquidGlass,
    FrostedGlass,
    Translucent,
    Paper,
    Ceramic,
    BrushedMetal,
}

impl SurfaceTreatment {
    pub const ALL: [Self; 6] = [
        Self::LiquidGlass,
        Self::FrostedGlass,
        Self::Translucent,
        Self::Paper,
        Self::Ceramic,
        Self::BrushedMetal,
    ];

    /// The default values of the treatment, used by `material`. Each blur, opacity and refraction is
    /// a choice of the visual design.
    #[must_use]
    pub const fn parameters(self) -> TreatmentParameters {
        let (blur, opacity, refraction) = match self {
            Self::LiquidGlass => (
                Some(constant!(NonNegativeDp::try_new(Dp::new(3.0)))),
                0.09,
                Some(LIQUID_GLASS_LENS.refraction),
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

    /// The material of the treatment with its default values, for a surface of `size` tinted with
    /// `tint`. `size` sets the extent of the gradients.
    #[must_use]
    pub fn material(self, tint: Color, size: Size) -> Material {
        self.material_with(tint, size, self.parameters())
    }

    /// The material of the treatment with `parameters` in place of its default values. A blur or a
    /// refraction that the treatment does not have is ignored. A blur or a refraction that the
    /// treatment has but `parameters` lack uses the default value of the treatment. Each color and
    /// offset is a choice of the visual design.
    #[must_use]
    pub fn material_with(
        self,
        tint: Color,
        size: Size,
        parameters: TreatmentParameters,
    ) -> Material {
        let white = Color::WHITE;
        let opacity = parameters.opacity;
        let shading = |stops: &[ColorStop]| {
            let kind = GradientKind::Linear {
                from: Point::new(0.dp(), 0.dp()),
                to: Point::new(size.width, size.height),
            };
            Gradient::new(kind, stops)
        };
        let base = Material::new(tint)
            .shadow(constant!(Shadow::new(
                Color::oklch(0.12, 0.015, 80.0).with_alpha(0.24),
                Dp::new(18.0),
                Point::new(Dp::ZERO, Dp::new(18.0)),
            )))
            .border(constant!(Stroke::new(
                Color::WHITE.with_alpha(0.7),
                Dp::new(1.0),
            )));
        let base = Material {
            backdrop: parameters.limited_to(self.parameters()).backdrop(),
            ..base
        };
        match self {
            Self::LiquidGlass => base.fill(tint.with_alpha(opacity)).shading(shading(&[
                ColorStop::new(0.0, white.with_alpha(0.19)),
                ColorStop::new(0.5, white.with_alpha(0.01)),
                ColorStop::new(1.0, white.with_alpha(0.12)),
            ])),
            Self::FrostedGlass => base.fill(tint.with_alpha(opacity)).shading(shading(&[
                ColorStop::new(0.0, white.with_alpha(0.24)),
                ColorStop::new(1.0, white.with_alpha(0.04)),
            ])),
            Self::Translucent => base.fill(tint.with_alpha(opacity)),
            Self::Paper => base
                .fill(Color::oklch(0.97, 0.016, 80.0).with_alpha(opacity))
                .border(constant!(Stroke::new(
                    Color::oklch(0.85, 0.025, 80.0),
                    Dp::new(1.0),
                ))),
            Self::Ceramic => base.fill(tint.with_alpha(opacity)).shading(shading(&[
                ColorStop::new(0.0, white.with_alpha(0.8)),
                ColorStop::new(0.5, white.with_alpha(0.32)),
                ColorStop::new(1.0, Color::oklch(0.25, 0.025, 80.0).with_alpha(0.18)),
            ])),
            Self::BrushedMetal => base
                .fill(Color::oklch(0.78, 0.014, 80.0).with_alpha(opacity))
                .shading(Gradient::new(
                    GradientKind::Linear {
                        from: Point::new(0.dp(), 0.dp()),
                        to: Point::new(size.width, 0.dp()),
                    },
                    [
                        ColorStop::new(0.0, white.with_alpha(0.15)),
                        ColorStop::new(1.0 / 3.0, white.with_alpha(0.65)),
                        ColorStop::new(2.0 / 3.0, Color::oklch(0.25, 0.01, 80.0).with_alpha(0.2)),
                        ColorStop::new(1.0, white.with_alpha(0.5)),
                    ],
                ))
                .shadow(
                    constant!(Shadow::new(
                        Color::oklch(0.15, 0.01, 80.0).with_alpha(0.2),
                        Dp::new(3.0),
                        Point::new(Dp::ZERO, Dp::new(2.0)),
                    ))
                    .inset(true),
                ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIZE: Size = Size::new(Dp::new(100.0), Dp::new(50.0));

    fn changed() -> TreatmentParameters {
        TreatmentParameters {
            blur: Some(constant!(NonNegativeDp::try_new(Dp::new(7.0)))),
            opacity: 0.5,
            refraction: Some(constant!(NonNegativeDp::try_new(Dp::new(5.0)))),
        }
    }

    #[test]
    fn a_treatment_builds_the_values_of_its_parameters() {
        let tint = Color::oklch(0.93, 0.035, 80.0);
        for treatment in SurfaceTreatment::ALL {
            let parameters = treatment.parameters();
            let backdrop = treatment.material(tint, SIZE).backdrop;
            assert_eq!(backdrop.map(|backdrop| backdrop.blur), parameters.blur);
            assert_eq!(
                backdrop.and_then(|backdrop| backdrop.glass.map(|glass| glass.refraction)),
                parameters.refraction
            );
        }
    }

    #[test]
    fn changed_parameters_are_built_where_the_treatment_has_them() {
        let tint = Color::oklch(0.93, 0.035, 80.0);
        let glass = SurfaceTreatment::LiquidGlass.material_with(tint, SIZE, changed());
        assert_eq!(glass.fill, Brush::from(tint.with_alpha(0.5)));
        assert_eq!(
            glass.backdrop,
            Some(constant!(Backdrop::glass(
                Dp::new(7.0),
                Glass {
                    refraction: constant!(NonNegativeDp::try_new(Dp::new(5.0))),
                    ..LIQUID_GLASS_LENS
                }
            )))
        );
        let frosted = SurfaceTreatment::FrostedGlass.material_with(tint, SIZE, changed());
        assert_eq!(
            frosted.backdrop,
            Some(constant!(Backdrop::blur(Dp::new(7.0))))
        );
    }

    #[test]
    fn a_parameter_that_the_treatment_lacks_is_ignored_and_one_that_it_has_is_kept() {
        let tint = Color::oklch(0.93, 0.035, 80.0);
        let paper = SurfaceTreatment::Paper.material_with(tint, SIZE, changed());
        assert_eq!(paper.backdrop, None);
        let lacking = TreatmentParameters {
            blur: None,
            refraction: None,
            ..changed()
        };
        let glass = SurfaceTreatment::LiquidGlass.material_with(tint, SIZE, lacking);
        assert_eq!(
            glass.backdrop,
            SurfaceTreatment::LiquidGlass.material(tint, SIZE).backdrop
        );
    }
}
