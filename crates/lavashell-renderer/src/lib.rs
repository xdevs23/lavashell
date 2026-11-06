// SPDX-License-Identifier: GPL-3.0-only

pub mod color;
mod compositing;
mod destination;
pub mod effect_registry;
mod effects;
mod error;
mod failure;
mod field;
mod fonts;
mod gpu_timing;
mod image;
mod input_region;
mod packet_cache;
pub mod paint;
mod plan;
mod raster;
mod repeated_leaves;
mod resources;
mod retention;
mod scalar;
mod silhouette;
pub mod source_filters;
mod state;
mod stream;
pub mod text;
mod text_alignment;
mod text_cache;
pub mod vulkan;

pub use compositing::BlendSpace;
pub use error::{ContextState, Error, FailureList, MultipleFailures, SkslDiagnostic};
pub use failure::DrawFailure;
pub use field::{FieldRoom, UniformRange};
pub use fonts::Fonts;
pub use image::{decode_image, encoded_image, raster_png};
pub use input_region::{InputSizeError, input_cells};
pub use packet_cache::{PacketCaching, SurfaceFrame};
pub use paint::{
    OutlineError, Painter, StreamFrames, StreamPixels, StreamRejection, UnsampledFormat, clip_path,
    outline_path, skpoint, skrect,
};
pub use skia_safe as skia;
pub use state::{
    Attach, AttachError, CommitError, DrawError, Drawn, FrameError, PainterSetup, PresentFrame,
    RendererSetup, RendererState, SceneSetup, StartError, StateError, SurfaceControl,
};
pub use stream::{
    BufferQueue, Completions, FenceCopyError, FrameDevice, FrameImporter, FrameSource, Latch,
    LatchAccess, LatchedFrames, NoStreams, ProducedFrame, Stream, StreamBackend, Taken,
};
pub use vulkan::{
    BorrowedImage, Completion, EmbeddedVulkanRenderer, ExternalImage, ForeignAlpha, ForeignFormat,
    ForeignImport, ForeignImporter, ForeignMemory, FrameOutcome, PresentationBackend,
    PresentationPolicy, ReadySignal, SampledMemory, Sampling, SurfaceAlpha, SurfaceContents,
    SurfaceTarget, SurfaceTiming, VulkanRenderer,
};

#[cfg(test)]
mod compositing_tests;
#[cfg(test)]
mod effects_tests;
#[cfg(test)]
mod float_asserts;
#[cfg(test)]
mod image_tests;
#[cfg(test)]
mod packet_cache_tests;
#[cfg(test)]
mod repeated_leaves_tests;
#[cfg(test)]
mod retained_tests;
#[cfg(test)]
mod surface_tests;
#[cfg(test)]
mod test_error;
#[cfg(test)]
mod test_font;
#[cfg(test)]
mod test_scenes;
#[cfg(test)]
mod test_surfaces;
