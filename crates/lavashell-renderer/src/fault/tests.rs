// SPDX-License-Identifier: GPL-3.0-only
use super::Fault;
use crate::DrawFailure;
use crate::error::{ContextState, Error, SkslDiagnostic};
use crate::fonts::{FontOrigin, RegisteredFont};
use crate::paint::StreamRejection;
use crate::test_error::TestError;
use crate::text::TextAntialiasing;
use ash::vk;
use lavashell_core::id_counter::IdsExhausted;
use lavashell_core::prelude::{NumberError, OutputProfile, PixelSize};
use lavashell_core::scene::FontWeight;
use lavashell_effect_quality::EffectId;
use std::num::TryFromIntError;
use std::time::Duration;

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

fn vulkan() -> lavashell_vulkan::Error {
    lavashell_vulkan::Error::MissingSwapchainMaintenance
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

const SIZE: PixelSize = PixelSize {
    width: 1,
    height: 1,
};

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
fn an_image_device_or_timer_error_shows_the_defect_notice_exactly_when_this_build_is_at_fault()
-> Result<(), TestError> {
    let not_utf8 = vec![u8::MAX];
    let utf8 =
        (std::str::from_utf8(&not_utf8).err()).ok_or(TestError::Unexpected("a UTF-8 error"))?;
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
        Error::InstanceExtensions(vulkan()),
        Error::Instance(vulkan()),
        Error::PhysicalDevices(vulkan()),
        Error::PhysicalDevice(vulkan()),
        Error::NoDevice,
        Error::Device(vulkan()),
        Error::NoDeviceLocalHeap,
        Error::ExtensionName(c"VK_KHR_surface", utf8),
        Error::SkiaContext,
        Error::SkiaHandoff(narrowing()?),
        Error::SkiaAbandoned,
        Error::CoverageSurface,
        Error::CoverageReadback,
        Error::CreateTimer(vulkan()),
        Error::StartTimer(vulkan()),
        Error::StopTimer(vulkan()),
        Error::ReadTimer(vulkan()),
        Error::ResetTimer(vulkan()),
    ]);
    Ok(())
}

#[test]
fn a_scene_stream_or_embedding_error_shows_the_defect_notice_exactly_when_this_build_is_at_fault() {
    assert_shows_the_defect_notice_exactly_when_this_build_is_at_fault(vec![
        Error::SubmitCompletion(vulkan()),
        Error::CompletionStatus(vulkan()),
        Error::ForeignExtensions(vulkan()),
        Error::ExportScene(vulkan()),
        Error::SceneFormat(vk::Format::UNDEFINED),
        Error::SceneSize(vk::Extent2D::default()),
        Error::ImportScene(vulkan()),
        Error::AcquireScene(vulkan()),
        Error::SceneInUse,
        Error::WrapScene,
        Error::HandBackScenes,
        Error::ReleaseScene(vulkan()),
        Error::ReleaseSceneStatus(vulkan()),
        Error::SceneMailboxFull,
        Error::StaleScene {
            generation: 1,
            published: 2,
        },
        Error::ForeignSize(SIZE),
        Error::ImportForeign(vulkan()),
        Error::WrapForeign,
        Error::HandBackForeign,
        Error::SignalRelease(vulkan()),
        Error::HostApiVersion(0),
        Error::ReadySemaphore(vulkan()),
        Error::EmbeddedFormat(vk::Format::UNDEFINED),
        Error::EmbeddedImageSize(SIZE),
        Error::EmbeddedImageHandle,
        Error::EmbeddedImageUsage(vk::ImageUsageFlags::empty()),
        Error::EmbeddedLayout(vk::ImageLayout::UNDEFINED),
        Error::WrapHostImage {
            format: vk::Format::UNDEFINED,
            usage: vk::ImageUsageFlags::empty(),
            size: SIZE,
            context: ContextState::Usable,
        },
        Error::EmbeddedReturnLayout(vk::ImageLayout::UNDEFINED),
        Error::WaitSemaphore,
        Error::ConsumeReadySignal(vulkan()),
        Error::EmbeddedSubmit,
    ]);
}

#[test]
fn a_surface_or_frame_error_shows_the_defect_notice_exactly_when_this_build_is_at_fault()
-> Result<(), TestError> {
    assert_shows_the_defect_notice_exactly_when_this_build_is_at_fault(vec![
        Error::CreateSurface(vulkan()),
        Error::QuerySurfacePresentation(vulkan()),
        Error::QuerySurfaceCapabilities(vulkan()),
        Error::QuerySurfaceFormats(vulkan()),
        Error::SurfacePresentation,
        Error::NoSurfaceFormat,
        Error::NoCompositeAlpha,
        Error::CreateSwapchain(vulkan()),
        Error::CreateSemaphore(vulkan()),
        Error::InvalidHdrMetadata,
        Error::HdrMetadataRange(DrawFailure::NumberRange),
        Error::SetHdrMetadata(vulkan()),
        Error::ForeignTarget,
        Error::WaitFrame(vulkan()),
        Error::Acquire(vulkan()),
        Error::SwapchainFormat(vk::Format::UNDEFINED),
        Error::SwapchainExtent(narrowing()?),
        Error::SwapchainImageIndex(narrowing()?),
        Error::WrapSwapchainImage(ContextState::DeviceLost),
        Error::WaitAcquisition,
        Error::WorkingColorSpace,
        Error::OutputColorSpace(OutputProfile::Srgb),
        Error::OutputColorFilter(DrawFailure::NumberRange),
        Error::WorkingSurface,
        Error::OffscreenSize(SIZE),
        Error::OffscreenSurface(SIZE),
        Error::PngSurface,
        Error::EncodePng,
        Error::ReadPixels,
        Error::PresentationSemaphore,
        Error::SubmitFrame,
        Error::SignalFrame(vulkan()),
        Error::Present(vulkan()),
        Error::NoDisplayTiming,
        Error::PresentationTimeRange(Duration::ZERO),
        Error::ExplicitTimestampRequired,
        Error::Several(vec![Error::WrapScene, Error::EncodePng]),
        Error::Several(vec![Error::WrapScene, Error::SubpixelTileSize]),
    ]);
    Ok(())
}
