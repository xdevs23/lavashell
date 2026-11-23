// SPDX-License-Identifier: GPL-3.0-only
use crate::failure::DrawFailure;
use crate::fonts::RegisteredFont;
use lavashell_core::prelude::NumberError;
use lavashell_core::scene::FontWeight;
use lavashell_effect_quality::EffectId;
use lavashell_error_chain::InOrder;
use lavashell_error_chain::next_step::{NamesNextStep, NextStep};
use skia_safe::gpu::DirectContextId;
use std::collections::BTreeMap;
use std::num::TryFromIntError;
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct SkslDiagnostic(pub(crate) String);

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("cannot read the font file {}", path.display())]
    ReadFont {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("cannot decode the {font}")]
    DecodeFont { font: RegisteredFont },
    #[error("cannot read the variation axes of the {font}")]
    ReadFontAxes { font: RegisteredFont },
    #[error("cannot instantiate the {font} at weight {}", .weight.get())]
    InstantiateFont {
        font: RegisteredFont,
        weight: FontWeight,
    },
    #[error("a font is registered twice under the alias {0}")]
    DuplicateFontAlias(String),
    #[error("the font alias {alias} is registered with an empty list of font files")]
    NoFontFiles { alias: String },
    #[error("cannot compile the {program} program")]
    CompileProgram {
        program: String,
        #[source]
        source: SkslDiagnostic,
    },
    #[error(
        "the {program} program declares the uniforms {declared:?} where the renderer writes {expected:?}"
    )]
    ProgramUniforms {
        program: String,
        declared: BTreeMap<String, usize>,
        expected: BTreeMap<String, usize>,
    },
    #[error(
        "the {program} program declares the children {declared:?} where the renderer passes {expected:?}"
    )]
    ProgramChildren {
        program: String,
        declared: Vec<String>,
        expected: Vec<String>,
    },
    #[error(
        "Skia finds no uniform {name} in the {program} program although the program declares it"
    )]
    MissingUniform { program: String, name: String },
    #[error("Skia finds no child {name} in the {program} program although the program declares it")]
    MissingChild { program: String, name: &'static str },
    #[error(
        "the uniform range of the device leaves the {program} program no room for the geometry it \
         draws"
    )]
    NoUniformRoom { program: String },
    #[error("the uniform range of the device lies beyond the address space of this platform")]
    UniformRangeSize(#[source] TryFromIntError),
    #[error("Skia reports a negative count for a uniform of the {program} program")]
    UniformCount {
        program: String,
        #[source]
        source: TryFromIntError,
    },
    #[error("the {effect} effect of subpixel text does not compile")]
    CompileSubpixelEffect {
        effect: &'static str,
        #[source]
        source: SkslDiagnostic,
    },
    #[error("cannot allocate the coverage tile of subpixel text")]
    SubpixelTile,
    #[error(
        "the coverage tile of subpixel text is empty or has more bytes than the resource cache allows, although \
         the renderer sized it to fit"
    )]
    SubpixelTileSize,
    #[error("Skia cannot sample the coverage mask of subpixel text")]
    SubpixelMask,
    #[error("Skia rejects the blender of subpixel text")]
    SubpixelBlender,
    #[error("subpixel text is drawn onto a surface whose color type Skia does not know")]
    SubpixelColorType,
    #[error("the blend tolerance of subpixel text lies beyond the range of a shader float")]
    SubpixelBlendTolerance(#[source] DrawFailure),
    #[error(
        "the scene image was imported by renderer {importer:?}, but the canvas belongs to {canvas_owner:?}"
    )]
    SceneOfAnotherRenderer {
        importer: DirectContextId,
        canvas_owner: Option<DirectContextId>,
    },
    #[error("the effect {0:?} is registered twice")]
    DuplicateEffect(EffectId),
    #[error("two text rasterizers are registered for {0:?} antialiasing")]
    DuplicateTextRasterizer(crate::text::TextAntialiasing),
    #[error("Skia cannot decode the image")]
    DecodeImage,
    #[error("the surface of a PNG image could not get an id")]
    NumberImageSurface(#[source] lavashell_core::id_counter::IdsExhausted),
    #[error("the decoded image has a negative size")]
    DecodedImageSize(#[source] TryFromIntError),
    #[error("Skia cannot read the header of the encoded image")]
    ReadImageHeader,
    #[error("the header of the encoded image states a negative size")]
    ImageHeaderSize(#[source] TryFromIntError),
    #[error("a raster export of {width}x{height} pixels is too large for Skia")]
    RasterSize { width: u32, height: u32 },
    #[error("Skia cannot allocate the surface of a raster export")]
    RasterSurface,
    #[error("the texels along a side of a field texture are no count")]
    TexelCount(#[source] NumberError),
    #[error("a field texture has more texels along a side than a Skia image holds")]
    TexelSide(#[source] TryFromIntError),
    #[error("cannot convert the count {count} for a shader uniform")]
    ShaderCount {
        count: usize,
        #[source]
        source: NumberError,
    },
    #[error("the count {count} for a shader uniform lies beyond the range of a shader float")]
    ShaderCountRange {
        count: usize,
        #[source]
        source: DrawFailure,
    },
    #[error("a shader float does not hold the count {0} exactly")]
    InexactShaderCount(usize),
    #[error("Skia does not provide the working color space")]
    WorkingColorSpace,
    #[error("the map from dp to output pixels lies beyond the range of Skia scalars")]
    OutputTransform(#[source] DrawFailure),
    #[error("Skia cannot allocate the surface of a PNG export")]
    PngSurface,
    #[error("Skia cannot encode a PNG export")]
    EncodePng,
    #[error("Skia cannot read the pixels of a canvas")]
    ReadPixels,
    #[error("{}", InOrder(.0))]
    Several(Vec<Error>),
}

impl NamesNextStep for Error {
    fn next_step(&self) -> NextStep {
        match self {
            Self::ReadFont { .. }
            | Self::DecodeFont { .. }
            | Self::ReadFontAxes { .. }
            | Self::InstantiateFont { .. } => NextStep::ReplaceTheFontFile,
            Self::NoUniformRoom { .. } | Self::UniformRangeSize(_) => {
                NextStep::UseSupportingPlatform
            }
            Self::SubpixelTile | Self::RasterSurface | Self::PngSurface => NextStep::FreeGpuMemory,
            Self::DecodeImage
            | Self::ReadImageHeader
            | Self::TexelCount(_)
            | Self::TexelSide(_)
            | Self::ShaderCount { .. }
            | Self::InexactShaderCount(_) => NextStep::ChangeTheScene,
            Self::SubpixelBlendTolerance(failure)
            | Self::ShaderCountRange {
                source: failure, ..
            }
            | Self::OutputTransform(failure) => failure.next_step(),
            Self::DuplicateFontAlias(_)
            | Self::NoFontFiles { .. }
            | Self::CompileProgram { .. }
            | Self::ProgramUniforms { .. }
            | Self::ProgramChildren { .. }
            | Self::MissingUniform { .. }
            | Self::MissingChild { .. }
            | Self::UniformCount { .. }
            | Self::CompileSubpixelEffect { .. }
            | Self::SubpixelTileSize
            | Self::SubpixelMask
            | Self::SubpixelBlender
            | Self::SubpixelColorType
            | Self::SceneOfAnotherRenderer { .. }
            | Self::DuplicateEffect(_)
            | Self::DuplicateTextRasterizer(_)
            | Self::NumberImageSurface(_)
            | Self::DecodedImageSize(_)
            | Self::ImageHeaderSize(_)
            | Self::RasterSize { .. }
            | Self::WorkingColorSpace
            | Self::EncodePng
            | Self::ReadPixels => NextStep::ReportTheFailure,
            Self::Several(failures) => failures.next_step(),
        }
    }
}

pub trait MultipleFailures: Sized {
    fn several(failures: Vec<Self>) -> Self;

    fn into_failures(self) -> Vec<Self>;
}

impl MultipleFailures for Error {
    fn several(failures: Vec<Self>) -> Self {
        Self::Several(failures)
    }

    fn into_failures(self) -> Vec<Self> {
        match self {
            Self::Several(failures) => failures,
            failure => vec![failure],
        }
    }
}

pub struct FailureList<E>(Vec<E>);

impl<E> Default for FailureList<E> {
    fn default() -> Self {
        Self(Vec::new())
    }
}

impl<E: MultipleFailures> FailureList<E> {
    pub fn add(&mut self, result: Result<(), E>) {
        if let Err(error) = result {
            self.push(error);
        }
    }

    pub fn push(&mut self, error: E) {
        self.0.extend(error.into_failures());
    }

    pub fn into_result(mut self) -> Result<(), E> {
        if self.0.len() > 1 {
            return Err(E::several(self.0));
        }
        self.0.pop().map_or(Ok(()), Err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_error::TestError;

    #[test]
    fn collected_failures_keep_their_order_and_a_single_one_stays_itself() -> Result<(), TestError>
    {
        let mut none = FailureList::<Error>::default();
        none.add(Ok(()));
        assert!(none.into_result().is_ok());
        let mut one = FailureList::default();
        one.add(Err(Error::ReadPixels));
        assert!(matches!(one.into_result(), Err(Error::ReadPixels)));
        let mut several = FailureList::default();
        several.push(Error::ReadPixels);
        several.push(Error::Several(vec![Error::EncodePng, Error::PngSurface]));
        let Err(Error::Several(errors)) = several.into_result() else {
            return Err(TestError::Unexpected("several failures"));
        };
        assert!(matches!(
            errors.as_slice(),
            [Error::ReadPixels, Error::EncodePng, Error::PngSurface]
        ));
        Ok(())
    }

    #[test]
    fn several_errors_take_the_next_step_of_their_failures() {
        let failures = vec![Error::RasterSurface, Error::SubpixelTileSize];
        let step = failures.next_step();
        assert_eq!(Error::Several(failures).next_step(), step);
    }

    #[test]
    fn an_error_that_wraps_a_draw_failure_takes_the_next_step_of_the_draw_failure() {
        let wrapped = DrawFailure::NumberRange;
        assert_eq!(
            Error::OutputTransform(wrapped).next_step(),
            wrapped.next_step()
        );
    }
}
