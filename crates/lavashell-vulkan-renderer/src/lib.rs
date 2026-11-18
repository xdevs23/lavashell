// SPDX-License-Identifier: GPL-3.0-only

mod backend;
mod color;
mod error;
mod state;
mod stream;
pub mod vulkan;

pub use backend::{HeadlessBackend, VulkanBackend, VulkanWindow};
pub use error::{ContextState, Error};
pub use state::{
    CommitError, DrawError, FrameError, RendererState, SceneSetup, StartError, StateError,
};
pub use stream::{NoStreams, RendererStreams};
pub use vulkan::{
    BorrowedImage, Completion, EmbeddedVulkanRenderer, ExternalImage, ForeignAlpha, ForeignFormat,
    ForeignImport, ForeignImporter, ForeignMemory, ForeignMemoryDescription, HeadlessPresentation,
    PresentationBackend, ReadySignal, SampledMemory, SurfaceTarget, VulkanRenderer,
};

#[cfg(test)]
mod painter_gpu_tests;
#[cfg(test)]
mod test_error;
