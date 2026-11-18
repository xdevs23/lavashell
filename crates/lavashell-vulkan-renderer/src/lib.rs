// SPDX-License-Identifier: GPL-3.0-only

mod color;
mod error;
mod state;
mod stream;
pub mod vulkan;

pub use error::{ContextState, Error};
pub use state::{
    Attach, AttachError, CommitError, DrawError, Drawn, FrameError, PainterSetup, PresentFrame,
    RendererSetup, RendererState, SceneSetup, StartError, StateError, SurfaceControl,
};
pub use stream::{NoStreams, RendererStreams};
pub use vulkan::{
    BorrowedImage, Completion, EmbeddedVulkanRenderer, ExternalImage, ForeignAlpha, ForeignFormat,
    ForeignImport, ForeignImporter, ForeignMemory, ForeignMemoryDescription, FrameOutcome,
    HeadlessPresentation, PresentationBackend, ReadySignal, SampledMemory, SurfaceAlpha,
    SurfaceContents, SurfaceTarget, VulkanRenderer,
};

#[cfg(test)]
mod painter_gpu_tests;
#[cfg(test)]
mod test_error;
