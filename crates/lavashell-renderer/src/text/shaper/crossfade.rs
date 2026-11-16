// SPDX-License-Identifier: GPL-3.0-only
use super::Shaper;
use lavashell_core::prelude::*;

#[derive(Debug, PartialEq)]
pub(crate) enum WeightRendering {
    OneFace,
    TwoFaces([FaceShare; 2]),
}

#[derive(Debug, PartialEq)]
pub(crate) struct FaceShare {
    pub weight: FontWeight,
    pub share_of_coverage: f64,
}

impl FaceShare {
    pub fn faded_style(&self, style: &TextStyle<Dp>) -> TextStyle<Dp> {
        TextStyle {
            weight: self.weight,
            color: style
                .color
                .with_alpha(style.color.alpha() * self.share_of_coverage),
            ..style.clone()
        }
    }
}

impl WeightRendering {
    fn between(face_weights: &[FontWeight], weight: FontWeight) -> Self {
        let faces = || face_weights.iter().copied();
        let lighter = faces().filter(|face| *face <= weight).max();
        let heavier = faces().filter(|face| *face >= weight).min();
        match (lighter, heavier) {
            (Some(lighter), Some(heavier)) if lighter != heavier => {
                let heavier_share =
                    (weight.get() - lighter.get()) / (heavier.get() - lighter.get());
                Self::TwoFaces([
                    FaceShare {
                        weight: lighter,
                        share_of_coverage: 1.0 - heavier_share,
                    },
                    FaceShare {
                        weight: heavier,
                        share_of_coverage: heavier_share,
                    },
                ])
            }
            _ => Self::OneFace,
        }
    }
}

impl Shaper {
    pub(crate) fn renderings_showing_weight(&mut self, style: &TextStyle<Dp>) -> WeightRendering {
        let face_weights = self.loaded_fonts.static_face_weights(&style.family);
        WeightRendering::between(face_weights, style.weight)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_error::TestError;
    use crate::test_font::probe_style_before_scaling;

    fn weight(value: f64) -> Result<FontWeight, TestError> {
        Ok(FontWeight::try_new(value)?)
    }

    #[test]
    fn a_weight_between_two_faces_shows_both_in_shares_of_its_distance_to_each()
    -> Result<(), TestError> {
        let faces = [FontWeight::NORMAL, FontWeight::BOLD];
        assert_eq!(
            WeightRendering::between(&faces, weight(475.0)?),
            WeightRendering::TwoFaces([
                FaceShare {
                    weight: FontWeight::NORMAL,
                    share_of_coverage: 0.75,
                },
                FaceShare {
                    weight: FontWeight::BOLD,
                    share_of_coverage: 0.25,
                },
            ])
        );
        Ok(())
    }

    #[test]
    fn a_weight_of_a_face_or_beyond_every_face_shows_one_face() -> Result<(), TestError> {
        let faces = [FontWeight::NORMAL, FontWeight::BOLD];
        let one_face = WeightRendering::OneFace;
        assert_eq!(WeightRendering::between(&faces, FontWeight::BOLD), one_face);
        assert_eq!(WeightRendering::between(&faces, weight(800.0)?), one_face);
        Ok(())
    }

    #[test]
    fn a_family_without_static_faces_shows_one_face() {
        assert_eq!(
            WeightRendering::between(&[], FontWeight::NORMAL),
            WeightRendering::OneFace
        );
    }

    #[test]
    fn a_weight_between_two_named_weights_on_a_font_with_a_weight_axis_shows_one_face()
    -> Result<(), TestError> {
        let fonts = crate::test_font::test_font()?.load()?;
        let mut shaper = Shaper::new(fonts, crate::failure::Failures::default());
        let style = probe_style_before_scaling()
            .size(16.sp())
            .weight(weight(550.0)?)
            .resolve_unscaled();
        assert_eq!(
            shaper.renderings_showing_weight(&style),
            WeightRendering::OneFace
        );
        assert_eq!(
            shaper.renderings_showing_weight(&style),
            WeightRendering::OneFace
        );
        Ok(())
    }

    #[test]
    fn a_faded_face_shows_its_share_of_the_coverage_in_the_alpha_of_the_text_color() {
        let face = FaceShare {
            weight: FontWeight::BOLD,
            share_of_coverage: 0.25,
        };
        let style =
            TextStyle::new("sans-serif", 16.sp(), Color::WHITE.with_alpha(0.8)).resolve_unscaled();
        let faded = face.faded_style(&style);
        assert_eq!(faded.weight, FontWeight::BOLD);
        assert_eq!(faded.color, Color::WHITE.with_alpha(0.2));
    }
}
