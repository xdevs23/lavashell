// SPDX-License-Identifier: GPL-3.0-only
use ash::vk;
use std::ffi::{CStr, FromBytesUntilNulError};
use std::num::TryFromIntError;
use std::str::Utf8Error;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("cannot list the instance extensions")]
    InstanceExtensions(#[source] vk::Result),
    #[error("cannot create the Vulkan instance")]
    CreateInstance(#[source] vk::Result),
    #[error("cannot list the physical devices")]
    PhysicalDevices(#[source] vk::Result),
    #[error("cannot list the extensions of the physical device")]
    DeviceExtensions(#[source] vk::Result),
    #[error("the name of the physical device is not terminated")]
    DeviceName(#[source] FromBytesUntilNulError),
    #[error("the name of the physical device is not UTF-8")]
    DeviceNameEncoding(#[source] Utf8Error),
    #[error(
        "the query needs Vulkan {required:#x}, and the instance or the device offers {available:#x}"
    )]
    ApiVersion { required: u32, available: u32 },
    #[error("the Vulkan instance or device lacks {0:?}")]
    MissingExtension(&'static CStr),
    #[error("the device lacks sampler YCbCr conversion")]
    MissingYcbcrConversion,
    #[error("the model {0:?} does not convert between YCbCr and RGB")]
    YcbcrModel(vk::SamplerYcbcrModelConversion),
    #[error("the YCbCr range {0:?} is unknown")]
    YcbcrRange(vk::SamplerYcbcrRange),
    #[error("the device has no queue family {0}")]
    QueueFamily(u32),
    #[error("cannot create the logical device")]
    CreateDevice(#[source] vk::Result),
    #[error("cannot wait until the device is idle")]
    WaitIdle(#[source] vk::Result),
    #[error("cannot create a fence")]
    CreateFence(#[source] vk::Result),
    #[error("cannot read the status of a fence")]
    FenceStatus(#[source] vk::Result),
    #[error("cannot wait for a fence")]
    WaitFence(#[source] vk::Result),
    #[error("cannot create a semaphore")]
    CreateSemaphore(#[source] vk::Result),
    #[error("cannot export a sync fd of a semaphore")]
    ExportSemaphore(#[source] vk::Result),
    #[error("cannot submit to the queue")]
    Submit(#[source] vk::Result),
    #[error("cannot create a command pool")]
    CreateCommandPool(#[source] vk::Result),
    #[error("cannot allocate a command buffer")]
    AllocateCommandBuffer(#[source] vk::Result),
    #[error("the driver allocated no command buffer")]
    NoCommandBuffer,
    #[error("cannot reset a command pool")]
    ResetCommandPool(#[source] vk::Result),
    #[error("cannot begin recording commands")]
    BeginCommands(#[source] vk::Result),
    #[error("cannot finish recording commands")]
    EndCommands(#[source] vk::Result),
    #[error("the commands do not know the state of the image")]
    UnknownImageState,
    #[error("an image cannot move into {0:?}")]
    TargetLayout(vk::ImageLayout),
    #[error("an image outside the queue cannot change its layout")]
    ExternalTransition,
    #[error("an image in {0:?} cannot be cleared")]
    ClearLayout(vk::ImageLayout),
    #[error("the object belongs to another device")]
    ForeignObject,
    #[error("the device cannot create the image")]
    ImageSupport(#[source] vk::Result),
    #[error("the device cannot share the memory of the image")]
    DeviceCannotShare,
    #[error("the image is larger than the device supports")]
    ImageExtent,
    #[error("cannot create an image")]
    CreateImage(#[source] vk::Result),
    #[error("no memory type suits the image")]
    NoMemoryType,
    #[error("cannot allocate memory")]
    AllocateMemory(#[source] vk::Result),
    #[error("cannot bind memory to an image")]
    BindMemory(#[source] vk::Result),
    #[error("cannot export the memory of an image")]
    ExportMemory(#[source] vk::Result),
    #[error("the driver exported the memory of an image without a file descriptor")]
    MemoryWithoutDescriptor,
    #[error("cannot duplicate the file descriptor of exported memory")]
    DuplicateDescriptor(#[source] std::io::Error),
    #[error("the exported memory belongs to another physical device")]
    DifferentDevice,
    #[error("the exported memory does not contain the image")]
    ExportedMemoryMismatch,
    #[error("the queue family of the device records no timestamps")]
    NoTimestamps,
    #[error("cannot create a query pool")]
    CreateQueryPool(#[source] vk::Result),
    #[error("cannot read the results of queries")]
    QueryResults(#[source] vk::Result),
    #[error("the window offers no handle")]
    WindowHandle(#[source] raw_window_handle::HandleError),
    #[error("surfaces are created from the raw handles of Wayland windows only")]
    UnsupportedWindowSystem,
    #[error("cannot create a surface")]
    CreateSurface(#[source] vk::Result),
    #[error("cannot query the surface")]
    SurfaceQuery(#[source] vk::Result),
    #[error("the queue cannot present to the surface")]
    SurfacePresentation,
    #[error("the surface does not support the requested {0:?}")]
    SwapchainRequest(SwapchainProperty),
    #[error("cannot create a swapchain")]
    CreateSwapchain(#[source] vk::Result),
    #[error("cannot list the images of a swapchain")]
    SwapchainImages(#[source] vk::Result),
    #[error("cannot acquire a swapchain image")]
    Acquire(#[source] vk::Result),
    #[error("the present time does not fit the nanoseconds of `VK_GOOGLE_display_timing`")]
    PresentTime(#[source] TryFromIntError),
    #[error("cannot present a swapchain image")]
    Present(#[source] vk::Result),
    #[error("the HDR metadata holds {0}, which lies beyond the range of a Vulkan float")]
    HdrMetadataRange(f64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SwapchainProperty {
    ImageCount,
    Format,
    Extent,
    Usage,
    Transform,
    CompositeAlpha,
    PresentMode,
}
