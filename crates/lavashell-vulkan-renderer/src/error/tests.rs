// SPDX-License-Identifier: GPL-3.0-only
use super::{ContextState, Error};
use crate::test_error::TestError;
use ash::vk;
use lavashell_core::prelude::{OutputProfile, PixelSize};
use lavashell_error_chain::message_chain;
use lavashell_renderer::DrawFailure;
use std::num::TryFromIntError;
use std::time::Duration;

const DEFECT_NOTICE: &str = "This is a defect in this build of the renderer";

const ONE_PIXEL: PixelSize = PixelSize {
    width: 1,
    height: 1,
};

fn count_defect_notices(error: &Error) -> usize {
    message_chain(error).matches(DEFECT_NOTICE).count()
}

fn vulkan_error() -> lavashell_vulkan::Error {
    lavashell_vulkan::Error::MissingSwapchainMaintenance
}

fn narrowing_error() -> Result<TryFromIntError, TestError> {
    (u8::try_from(u16::MAX).err()).ok_or(TestError::Unexpected("a narrowing error"))
}

#[test]
fn an_error_of_the_vulkan_renderer_does_not_show_the_defect_notice() -> Result<(), TestError> {
    let not_utf8 = vec![u8::MAX];
    let utf8_error =
        (std::str::from_utf8(&not_utf8).err()).ok_or(TestError::Unexpected("a UTF-8 error"))?;
    for error in [
        Error::InstanceExtensions(vulkan_error()),
        Error::Instance(vulkan_error()),
        Error::PhysicalDevices(vulkan_error()),
        Error::PhysicalDevice(vulkan_error()),
        Error::NoDevice,
        Error::Device(vulkan_error()),
        Error::NoDeviceLocalHeap,
        Error::ExtensionName(c"VK_KHR_surface", utf8_error),
        Error::SkiaContext,
        Error::SkiaHandoff(narrowing_error()?),
        Error::SkiaAbandoned,
        Error::CoverageSurface,
        Error::CoverageReadback,
        Error::CreateTimer(vulkan_error()),
        Error::StartTimer(vulkan_error()),
        Error::StopTimer(vulkan_error()),
        Error::ReadTimer(vulkan_error()),
        Error::ResetTimer(vulkan_error()),
        Error::SubmitCompletion(vulkan_error()),
        Error::CompletionStatus(vulkan_error()),
        Error::ForeignExtensions(vulkan_error()),
        Error::ExportScene(vulkan_error()),
        Error::SceneFormat(vk::Format::UNDEFINED),
        Error::SceneSize(vk::Extent2D::default()),
        Error::ImportScene(vulkan_error()),
        Error::AcquireScene(vulkan_error()),
        Error::SceneInUse,
        Error::WrapScene,
        Error::HandBackScenes,
        Error::ReleaseScene(vulkan_error()),
        Error::ReleaseSceneStatus(vulkan_error()),
        Error::SceneMailboxFull,
        Error::StaleScene {
            generation: 1,
            published: 2,
        },
        Error::ForeignSize(ONE_PIXEL),
        Error::ImportForeign(vulkan_error()),
        Error::WrapForeign,
        Error::HandBackForeign,
        Error::SignalRelease(vulkan_error()),
        Error::HostApiVersion(0),
        Error::ReadySemaphore(vulkan_error()),
        Error::EmbeddedFormat(vk::Format::UNDEFINED),
        Error::EmbeddedImageSize(ONE_PIXEL),
        Error::EmbeddedImageHandle,
        Error::EmbeddedImageUsage(vk::ImageUsageFlags::empty()),
        Error::EmbeddedLayout(vk::ImageLayout::UNDEFINED),
        Error::WrapHostImage {
            format: vk::Format::UNDEFINED,
            usage: vk::ImageUsageFlags::empty(),
            size: ONE_PIXEL,
            context: ContextState::Usable,
        },
        Error::EmbeddedReturnLayout(vk::ImageLayout::UNDEFINED),
        Error::WaitSemaphore,
        Error::ConsumeReadySignal(vulkan_error()),
        Error::EmbeddedSubmit,
        Error::CreateSurface(vulkan_error()),
        Error::QuerySurfacePresentation(vulkan_error()),
        Error::QuerySurfaceCapabilities(vulkan_error()),
        Error::QuerySurfaceFormats(vulkan_error()),
        Error::SurfacePresentation,
        Error::NoSurfaceFormat,
        Error::NoCompositeAlpha,
        Error::CreateSwapchain(vulkan_error()),
        Error::CreateSemaphore(vulkan_error()),
        Error::InvalidHdrMetadata,
        Error::HdrMetadataRange(DrawFailure::NumberRange),
        Error::SetHdrMetadata(vulkan_error()),
        Error::ForeignTarget,
        Error::WaitFrame(vulkan_error()),
        Error::Acquire(vulkan_error()),
        Error::SwapchainExtent(narrowing_error()?),
        Error::SwapchainImageIndex(narrowing_error()?),
        Error::WrapSwapchainImage(ContextState::DeviceLost),
        Error::WaitAcquisition,
        Error::OutputColorSpace(OutputProfile::Srgb),
        Error::OutputColorFilter(DrawFailure::NumberRange),
        Error::WorkingSurface,
        Error::OffscreenSize(ONE_PIXEL),
        Error::OffscreenSurface(ONE_PIXEL),
        Error::PngSurface,
        Error::PresentationSemaphore,
        Error::SubmitFrame,
        Error::SignalFrame(vulkan_error()),
        Error::Present(vulkan_error()),
        Error::NoDisplayTiming,
        Error::PresentationTimeRange(Duration::ZERO),
        Error::ExplicitTimestampRequired,
        Error::Several(vec![Error::WrapScene, Error::PngSurface]),
    ] {
        assert_eq!(count_defect_notices(&error), 0, "{error:?}");
    }
    Ok(())
}

#[test]
fn a_renderer_defect_inside_an_error_of_the_vulkan_renderer_shows_the_defect_notice_once() {
    let renderer_defect = || lavashell_renderer::Error::SubpixelTileSize;
    for error in [
        Error::CreatePainter(renderer_defect()),
        Error::WorkingImageInfo(renderer_defect()),
        Error::ReadPngPixels(renderer_defect()),
        Error::EncodePng(renderer_defect()),
        Error::Several(vec![
            Error::WrapScene,
            Error::CreatePainter(renderer_defect()),
        ]),
    ] {
        assert_eq!(count_defect_notices(&error), 1, "{error:?}");
    }
}
