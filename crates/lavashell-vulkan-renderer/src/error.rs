// SPDX-License-Identifier: GPL-3.0-only
use ash::vk;
use lavashell_core::prelude::{OutputProfile, PixelSize};
use lavashell_error_chain::InOrder;
use lavashell_error_chain::next_step::{NamesNextStep, NextStep};
use lavashell_renderer::{DrawFailure, MultipleFailures};
use std::ffi::CStr;
use std::num::TryFromIntError;
use std::str::Utf8Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContextState {
    Usable,
    DeviceLost,
    OutOfMemory,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("cannot create a painter")]
    CreatePainter(#[source] lavashell_renderer::Error),
    #[error("cannot describe the floating-point working surface")]
    WorkingImageInfo(#[source] lavashell_renderer::Error),
    #[error("cannot read the pixels for a PNG file")]
    ReadPngPixels(#[source] lavashell_renderer::Error),
    #[error("cannot encode a PNG file")]
    EncodePng(#[source] lavashell_renderer::Error),
    #[error("cannot list the Vulkan instance extensions")]
    InstanceExtensions(#[source] lavashell_vulkan::Error),
    #[error("cannot create the Vulkan instance")]
    Instance(#[source] lavashell_vulkan::Error),
    #[error("cannot list the Vulkan physical devices")]
    PhysicalDevices(#[source] lavashell_vulkan::Error),
    #[error("cannot query a Vulkan physical device")]
    PhysicalDevice(#[source] lavashell_vulkan::Error),
    #[error("cannot find a Vulkan 1.1 device with a graphics queue for the renderer")]
    NoDevice,
    #[error("cannot create the Vulkan device")]
    Device(#[source] lavashell_vulkan::Error),
    #[error("cannot find a device-local memory heap on the Vulkan device")]
    NoDeviceLocalHeap,
    #[error("the Vulkan extension name {0:?} is not UTF-8")]
    ExtensionName(&'static CStr, #[source] Utf8Error),
    #[error("Skia cannot create a context on the Vulkan device")]
    SkiaContext,
    #[error("a Vulkan handle, version or queue family does not fit the matching Skia type")]
    SkiaHandoff(#[source] TryFromIntError),
    #[error("Skia abandoned its context when the Vulkan device was lost, and submits nothing")]
    SkiaAbandoned,
    #[error("cannot allocate the surface that tests mask coverage on the device")]
    CoverageSurface,
    #[error("cannot read back the surface that tests mask coverage on the device")]
    CoverageReadback,
    #[error("cannot create a GPU timer")]
    CreateTimer(#[source] lavashell_vulkan::Error),
    #[error("cannot start a GPU timer")]
    StartTimer(#[source] lavashell_vulkan::Error),
    #[error("cannot stop a GPU timer")]
    StopTimer(#[source] lavashell_vulkan::Error),
    #[error("cannot read a GPU timer")]
    ReadTimer(#[source] lavashell_vulkan::Error),
    #[error("cannot reset a GPU timer")]
    ResetTimer(#[source] lavashell_vulkan::Error),
    #[error("cannot submit a signal that GPU work has completed")]
    SubmitCompletion(#[source] lavashell_vulkan::Error),
    #[error("cannot read whether GPU work has completed")]
    CompletionStatus(#[source] lavashell_vulkan::Error),
    #[error("the Vulkan device cannot import foreign memory")]
    ForeignExtensions(#[source] lavashell_vulkan::Error),
    #[error("cannot export a scene image")]
    ExportScene(#[source] lavashell_vulkan::Error),
    #[error("an imported scene image of {0:?} cannot be sampled as sRGB RGBA")]
    SceneFormat(vk::Format),
    #[error("an imported scene image of {0:?} is too large for Skia")]
    SceneSize(vk::Extent2D),
    #[error("cannot import a scene image")]
    ImportScene(#[source] lavashell_vulkan::Error),
    #[error("cannot acquire a scene image from the producer that made it")]
    AcquireScene(#[source] lavashell_vulkan::Error),
    #[error("the scene image is imported again before the renderer handed it back")]
    SceneInUse,
    #[error("Skia cannot wrap an imported scene image")]
    WrapScene,
    #[error(
        "Skia cannot move imported scene images into the layout that releasing them needs. This happens after \
         the Vulkan device is lost."
    )]
    HandBackScenes,
    #[error("cannot hand a scene image back to the producer that made it")]
    ReleaseScene(#[source] lavashell_vulkan::Error),
    #[error("cannot read whether a scene image went back to the producer that made it")]
    ReleaseSceneStatus(#[source] lavashell_vulkan::Error),
    #[error("the scene mailbox is full with pending scenes")]
    SceneMailboxFull,
    #[error("scene generation {generation} is not newer than the published generation {published}")]
    StaleScene { generation: u64, published: u64 },
    #[error("foreign memory of {0:?} is too large for Skia")]
    ForeignSize(PixelSize),
    #[error("cannot import foreign memory")]
    ImportForeign(#[source] lavashell_vulkan::Error),
    #[error("Skia cannot wrap imported foreign memory")]
    WrapForeign,
    #[error(
        "Skia did not hand the frames of streams back to the producers that made them. This happens after the \
         Vulkan device is lost."
    )]
    HandBackForeign,
    #[error("cannot signal the release fence of the frames of streams")]
    SignalRelease(#[source] lavashell_vulkan::Error),
    #[error("embedded rendering needs Vulkan 1.1, and the host instance has {0:#x}")]
    HostApiVersion(u32),
    #[error("cannot create the semaphore that orders the host before the embedded renderer")]
    ReadySemaphore(#[source] lavashell_vulkan::Error),
    #[error("the embedded renderer cannot draw into {0:?}")]
    EmbeddedFormat(vk::Format),
    #[error("the embedded renderer cannot draw into an image of {0:?}")]
    EmbeddedImageSize(PixelSize),
    #[error("the host image to draw into is null")]
    EmbeddedImageHandle,
    #[error(
        "the host image has {0:?}, and the embedded renderer needs color attachment and both transfer usages"
    )]
    EmbeddedImageUsage(vk::ImageUsageFlags),
    #[error("Skia cannot use a host image in {0:?}")]
    EmbeddedLayout(vk::ImageLayout),
    #[error(
        "Skia cannot wrap the host image of {format:?} with {usage:?} at {size:?}; its context is {context:?}"
    )]
    WrapHostImage {
        format: vk::Format,
        usage: vk::ImageUsageFlags,
        size: PixelSize,
        context: ContextState,
    },
    #[error("Skia cannot hand the host image back in {0:?}")]
    EmbeddedReturnLayout(vk::ImageLayout),
    #[error("Skia could not add a wait on the semaphore that the embedded renderer draws after")]
    WaitSemaphore,
    #[error(
        "cannot wait on the ready semaphore when the embedded renderer fails to draw, so the semaphore is still signaled"
    )]
    ConsumeReadySignal(#[source] lavashell_vulkan::Error),
    #[error("Skia cannot submit the frame that the embedded renderer draws")]
    EmbeddedSubmit,
    #[error("cannot create a Vulkan surface for the window")]
    CreateSurface(#[source] lavashell_vulkan::Error),
    #[error("cannot ask whether the graphics queue can present to the Vulkan surface")]
    QuerySurfacePresentation(#[source] lavashell_vulkan::Error),
    #[error("cannot read the capabilities of the Vulkan surface")]
    QuerySurfaceCapabilities(#[source] lavashell_vulkan::Error),
    #[error("cannot list the formats of the Vulkan surface")]
    QuerySurfaceFormats(#[source] lavashell_vulkan::Error),
    #[error("the graphics queue cannot present to the surface")]
    SurfacePresentation,
    #[error("cannot find an SDR sRGB format on the surface")]
    NoSurfaceFormat,
    #[error("cannot find a composite alpha mode that the target can use on the surface")]
    NoCompositeAlpha,
    #[error("cannot create the swapchain")]
    CreateSwapchain(#[source] lavashell_vulkan::Error),
    #[error("cannot create a semaphore of the swapchain")]
    CreateSemaphore(#[source] lavashell_vulkan::Error),
    #[error("the HDR mastering metadata is invalid")]
    InvalidHdrMetadata,
    #[error("the HDR mastering metadata lies beyond the range of a Vulkan float")]
    HdrMetadataRange(#[source] DrawFailure),
    #[error("cannot set the HDR mastering metadata of the swapchain")]
    SetHdrMetadata(#[source] lavashell_vulkan::Error),
    #[error("the surface target belongs to another renderer")]
    ForeignTarget,
    #[error("cannot wait for the previous frame of a swapchain slot")]
    WaitFrame(#[source] lavashell_vulkan::Error),
    #[error("cannot acquire a swapchain image")]
    Acquire(#[source] lavashell_vulkan::Error),
    #[error("the swapchain extent is too large for Skia")]
    SwapchainExtent(#[source] std::num::TryFromIntError),
    #[error(
        "the index of the acquired swapchain image lies beyond the address space of this platform"
    )]
    SwapchainImageIndex(#[source] TryFromIntError),
    #[error("Skia cannot wrap the swapchain image; its context is {0:?}")]
    WrapSwapchainImage(ContextState),
    #[error("Skia cannot wait until the swapchain image is acquired")]
    WaitAcquisition,
    #[error("Skia does not provide the color space of {0:?}")]
    OutputColorSpace(OutputProfile),
    #[error("the gain that maps the scene to scRGB lies beyond the range of a Skia scalar")]
    OutputColorFilter(#[source] DrawFailure),
    #[error("cannot allocate the floating-point working surface")]
    WorkingSurface,
    #[error("an offscreen surface of {0:?} is too large for Skia")]
    OffscreenSize(PixelSize),
    #[error("Skia cannot allocate an offscreen surface of {0:?}")]
    OffscreenSurface(PixelSize),
    #[error("Skia cannot allocate the GPU surface for a PNG file")]
    PngSurface,
    #[error("Skia did not submit the semaphore that presentation waits for")]
    PresentationSemaphore,
    #[error("Skia cannot submit the frame")]
    SubmitFrame,
    #[error("cannot submit a signal that a frame has completed")]
    SignalFrame(#[source] lavashell_vulkan::Error),
    #[error("cannot present a swapchain image")]
    Present(#[source] lavashell_vulkan::Error),
    #[error("the Vulkan device does not support VK_GOOGLE_display_timing")]
    NoDisplayTiming,
    #[error("the presentation time {0:?} is outside 1..=i64::MAX nanoseconds")]
    PresentationTimeRange(std::time::Duration),
    #[error("the surface requires an explicit timestamp on every frame after the first")]
    ExplicitTimestampRequired,
    #[error("{}", InOrder(.0))]
    Several(Vec<Error>),
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

impl NamesNextStep for ContextState {
    fn next_step(&self) -> NextStep {
        match self {
            Self::Usable => NextStep::ReportTheFailure,
            Self::DeviceLost => NextStep::UpdateGraphicsDriver,
            Self::OutOfMemory => NextStep::FreeGpuMemory,
        }
    }
}

impl NamesNextStep for Error {
    fn next_step(&self) -> NextStep {
        match self {
            Self::CreatePainter(error)
            | Self::WorkingImageInfo(error)
            | Self::ReadPngPixels(error)
            | Self::EncodePng(error) => error.next_step(),
            Self::InstanceExtensions(error)
            | Self::Instance(error)
            | Self::PhysicalDevices(error)
            | Self::PhysicalDevice(error)
            | Self::Device(error)
            | Self::CreateTimer(error)
            | Self::StartTimer(error)
            | Self::StopTimer(error)
            | Self::ReadTimer(error)
            | Self::ResetTimer(error)
            | Self::SubmitCompletion(error)
            | Self::CompletionStatus(error)
            | Self::ForeignExtensions(error)
            | Self::ExportScene(error)
            | Self::ImportScene(error)
            | Self::AcquireScene(error)
            | Self::ReleaseScene(error)
            | Self::ReleaseSceneStatus(error)
            | Self::ImportForeign(error)
            | Self::SignalRelease(error)
            | Self::ReadySemaphore(error)
            | Self::ConsumeReadySignal(error)
            | Self::CreateSurface(error)
            | Self::QuerySurfacePresentation(error)
            | Self::QuerySurfaceCapabilities(error)
            | Self::QuerySurfaceFormats(error)
            | Self::CreateSwapchain(error)
            | Self::CreateSemaphore(error)
            | Self::SetHdrMetadata(error)
            | Self::WaitFrame(error)
            | Self::Acquire(error)
            | Self::SignalFrame(error)
            | Self::Present(error) => error.next_step(),
            Self::HdrMetadataRange(failure) | Self::OutputColorFilter(failure) => {
                failure.next_step()
            }
            Self::WrapHostImage { context, .. } | Self::WrapSwapchainImage(context) => {
                context.next_step()
            }
            Self::CoverageSurface
            | Self::WorkingSurface
            | Self::OffscreenSize(_)
            | Self::OffscreenSurface(_)
            | Self::PngSurface => NextStep::FreeGpuMemory,
            Self::NoDevice
            | Self::NoDeviceLocalHeap
            | Self::SkiaContext
            | Self::SkiaAbandoned
            | Self::CoverageReadback
            | Self::WrapScene
            | Self::HandBackScenes
            | Self::WrapForeign
            | Self::HandBackForeign
            | Self::HostApiVersion(_)
            | Self::WaitSemaphore
            | Self::EmbeddedSubmit
            | Self::SurfacePresentation
            | Self::NoSurfaceFormat
            | Self::NoCompositeAlpha
            | Self::WaitAcquisition
            | Self::PresentationSemaphore
            | Self::SubmitFrame
            | Self::NoDisplayTiming => NextStep::UpdateGraphicsDriver,
            Self::ForeignSize(_) => NextStep::ChangeTheStreamProducer,
            Self::ExtensionName(..)
            | Self::SkiaHandoff(_)
            | Self::SceneFormat(_)
            | Self::SceneSize(_)
            | Self::SceneInUse
            | Self::SceneMailboxFull
            | Self::StaleScene { .. }
            | Self::EmbeddedFormat(_)
            | Self::EmbeddedImageSize(_)
            | Self::EmbeddedImageHandle
            | Self::EmbeddedImageUsage(_)
            | Self::EmbeddedLayout(_)
            | Self::EmbeddedReturnLayout(_)
            | Self::InvalidHdrMetadata
            | Self::ForeignTarget
            | Self::SwapchainExtent(_)
            | Self::SwapchainImageIndex(_)
            | Self::OutputColorSpace(_)
            | Self::PresentationTimeRange(_)
            | Self::ExplicitTimestampRequired => NextStep::ReportTheFailure,
            Self::Several(failures) => failures.next_step(),
        }
    }
}

#[cfg(test)]
mod tests;
