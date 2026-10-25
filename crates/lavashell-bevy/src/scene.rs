// SPDX-License-Identifier: GPL-3.0-only
//! Exports the images rendered by Bevy cameras to a consumer on another Vulkan device through external memory. The
//! images never leave the GPU. The consumer reads each image under a lease, and the producer does not render into a
//! leased image.

mod bridge;
mod export;
mod producer;
mod transfer;

pub use bridge::{BridgeStats, CompletedFrame, SceneBridge};
pub use producer::{
    SceneCamera, SceneExportSet, SceneProducerPlugin, ScenePublication, SceneSize, drain_app,
    poll_app,
};

/// The format in which the scene importer of the renderer reads exported images.
const SCENE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

/// The texture usages of a scene target: a camera renders into the target, the export clears the target and copies
/// out of it, and the consumer samples it.
const SCENE_USAGE: wgpu::TextureUsages = wgpu::TextureUsages::RENDER_ATTACHMENT
    .union(wgpu::TextureUsages::TEXTURE_BINDING)
    .union(wgpu::TextureUsages::COPY_SRC)
    .union(wgpu::TextureUsages::COPY_DST);
