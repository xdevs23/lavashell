// SPDX-License-Identifier: GPL-3.0-only
//! Owning wrappers around Vulkan objects, used by the renderer and the Bevy integration.
//!
//! Each `// SAFETY:` comment in this crate states why ash 0.38 offers no safe form of the call.

mod commands;
mod device;
mod error;
mod export;
mod extensions;
mod fd;
mod image;
mod imported_memory;
mod instance;
mod ownership;
mod physical;
mod submission;
mod surface;
mod swapchain;
mod sync;
mod timer;
pub mod ycbcr;

#[cfg(test)]
mod tests;

pub use commands::{Commands, ImageState, QueueFamily, Recorded, Recording, Submitted};
pub use device::{Device, DeviceRequest, HostDevice};
pub use error::{Error, SwapchainProperty};
pub use export::{ExportableImage, OpaqueFdExport};
pub use extensions::{ExtensionSet, Extensions};
pub use image::{Image, ImageDescription, ImageFormat};
pub use imported_memory::ImportedMemory;
pub use instance::{Instance, InstanceRequest};
pub use physical::{Features, HeapBudget, PhysicalDevice};
pub use submission::{Submission, SyncFdSignal};
pub use surface::{Presentation, Surface, Window, WindowSystem};
pub use swapchain::{
    Acquisition, Chromaticity, Fit, MasteringDisplay, Presented, Swapchain, SwapchainRequest,
};
pub use sync::{Semaphore, Status};
pub use timer::{GpuTimer, Reading, RunningTimer, StoppedTimer};
