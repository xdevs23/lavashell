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
pub mod text;
mod text_alignment;
mod text_cache;
pub mod vulkan;

pub use compositing::BlendSpace;
pub use error::{ContextState, Error, SkslDiagnostic};
pub use failure::DrawFailure;
pub use field::{FieldRoom, UniformRange};
pub use fonts::Fonts;
pub use gpu_timing::GpuFrameTime;
pub use image::{decode_image, encoded_image, raster_png};
pub use packet_cache::{PacketCaching, RenderStats, SurfaceFrame};
pub use paint::{
    BufferAlpha, BufferFlag, HardwareBufferDescription, Painter, StreamFrames, StreamPixels,
    StreamRejection, UnsampledFormat, skpoint, skrect,
};
pub use skia_safe as skia;
#[cfg(target_os = "linux")]
pub use vulkan::WaylandTarget;
pub use vulkan::{
    BorrowedImage, EmbeddedVulkanRenderer, ExternalImage, FrameOutcome, PresentationPolicy,
    ReadySignal, SurfaceAlpha, SurfaceContents, SurfaceTarget, SurfaceTiming, VulkanRenderer,
};
#[cfg(target_os = "android")]
pub use vulkan::{
    Completion, HardwareBufferImport, HardwareBufferImporter, HardwareBufferProperties, Sampling,
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
