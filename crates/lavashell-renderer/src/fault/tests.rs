// SPDX-License-Identifier: GPL-3.0-only
use super::Fault;
use crate::DrawFailure;
use crate::error::{Error, SkslDiagnostic};
use crate::fonts::{FontOrigin, RegisteredFont};
use crate::paint::StreamRejection;
use crate::test_error::TestError;
use crate::text::TextAntialiasing;
use lavashell_core::id_counter::IdsExhausted;
use lavashell_core::prelude::NumberError;
use lavashell_core::scene::FontWeight;
use lavashell_effect_quality::EffectId;
use std::num::TryFromIntError;

const DEFECT_NOTICE: &str = "This is a defect in this build of the renderer";

#[test]
fn a_draw_failure_shows_the_defect_notice_exactly_when_this_build_is_at_fault() {
    for failure in [
        DrawFailure::Allocation,
        DrawFailure::SurfaceRead,
        DrawFailure::LensWithoutSurface,
        DrawFailure::UnbalancedRestore,
        DrawFailure::ImageSize,
        DrawFailure::ImageUndecodable,
        DrawFailure::ImagePixels,
        DrawFailure::StreamUnbound,
        DrawFailure::StreamStale,
        DrawFailure::StreamRejected(StreamRejection::TooLarge),
        DrawFailure::ShaderRejected,
        DrawFailure::GpuTimestamp,
        DrawFailure::ColorSpace,
        DrawFailure::SingularTransform,
        DrawFailure::OutlineLength,
        DrawFailure::TileSize,
        DrawFailure::SubpixelText,
        DrawFailure::NumberRange,
    ] {
        assert_eq!(
            failure.to_string().contains(DEFECT_NOTICE),
            failure.fault() == Fault::DefectInThisBuild,
            "{failure:?}"
        );
    }
}

fn assert_shows_the_defect_notice_exactly_when_this_build_is_at_fault(errors: Vec<Error>) {
    for error in errors {
        assert_eq!(
            error.to_string().contains(DEFECT_NOTICE),
            error.fault() == Fault::DefectInThisBuild,
            "{error:?}"
        );
    }
}

fn narrowing() -> Result<TryFromIntError, TestError> {
    (u8::try_from(u16::MAX).err()).ok_or(TestError::Unexpected("a narrowing error"))
}

fn font() -> RegisteredFont {
    RegisteredFont {
        alias: "sans".to_owned(),
        origin: FontOrigin::Embedded,
    }
}

fn program() -> String {
    "glass".to_owned()
}

fn diagnostic() -> SkslDiagnostic {
    SkslDiagnostic(String::new())
}

#[test]
fn a_font_program_or_text_error_shows_the_defect_notice_exactly_when_this_build_is_at_fault()
-> Result<(), TestError> {
    assert_shows_the_defect_notice_exactly_when_this_build_is_at_fault(vec![
        Error::ReadFont {
            path: "sans.ttf".into(),
            source: std::io::Error::other("unreadable"),
        },
        Error::DecodeFont { font: font() },
        Error::ReadFontAxes { font: font() },
        Error::InstantiateFont {
            font: font(),
            weight: FontWeight::THIN,
        },
        Error::DuplicateFontAlias("sans".to_owned()),
        Error::NoFontFiles {
            alias: "sans".to_owned(),
        },
        Error::CompileProgram {
            program: program(),
            source: diagnostic(),
        },
        Error::ProgramUniforms {
            program: program(),
            declared: [("size".to_owned(), 1)].into(),
            expected: [("size".to_owned(), 2)].into(),
        },
        Error::ProgramChildren {
            program: program(),
            declared: vec!["backdrop".to_owned()],
            expected: Vec::new(),
        },
        Error::MissingUniform {
            program: program(),
            name: "size".to_owned(),
        },
        Error::MissingChild {
            program: program(),
            name: "backdrop",
        },
        Error::NoUniformRoom { program: program() },
        Error::UniformRangeSize(narrowing()?),
        Error::UniformCount {
            program: program(),
            source: narrowing()?,
        },
        Error::CompileSubpixelEffect {
            effect: "coverage",
            source: diagnostic(),
        },
        Error::SubpixelTile,
        Error::SubpixelTileSize,
        Error::SubpixelMask,
        Error::SubpixelBlender,
        Error::SubpixelColorType,
        Error::SubpixelBlendTolerance(DrawFailure::NumberRange),
        Error::DuplicateEffect(EffectId(1)),
        Error::DuplicateTextRasterizer(TextAntialiasing::Subpixel),
    ]);
    Ok(())
}

#[test]
fn an_image_or_shader_count_error_shows_the_defect_notice_exactly_when_this_build_is_at_fault()
-> Result<(), TestError> {
    assert_shows_the_defect_notice_exactly_when_this_build_is_at_fault(vec![
        Error::DecodeImage,
        Error::NumberImageSurface(IdsExhausted),
        Error::DecodedImageSize(narrowing()?),
        Error::ReadImageHeader,
        Error::ImageHeaderSize(narrowing()?),
        Error::RasterSize {
            width: 1,
            height: 1,
        },
        Error::RasterSurface,
        Error::TexelCount(NumberError::Negative),
        Error::TexelSide(narrowing()?),
        Error::ShaderCount {
            count: 1,
            source: NumberError::OutOfRange,
        },
        Error::ShaderCountRange {
            count: 1,
            source: DrawFailure::NumberRange,
        },
        Error::InexactShaderCount(1),
    ]);
    Ok(())
}

#[test]
fn a_color_space_or_export_error_shows_the_defect_notice_exactly_when_this_build_is_at_fault() {
    assert_shows_the_defect_notice_exactly_when_this_build_is_at_fault(vec![
        Error::WorkingColorSpace,
        Error::PngSurface,
        Error::EncodePng,
        Error::ReadPixels,
        Error::Several(vec![Error::ReadPixels, Error::EncodePng]),
        Error::Several(vec![Error::ReadPixels, Error::SubpixelTileSize]),
    ]);
}
