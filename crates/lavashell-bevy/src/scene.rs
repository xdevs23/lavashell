// SPDX-License-Identifier: GPL-3.0-only

mod bridge;
mod export;
mod producer;
mod transfer;

pub use bridge::{BridgeStats, CompletedFrame, SceneBridge};
pub use producer::{
    SceneCamera, SceneExportSet, SceneProducerPlugin, ScenePublication, SceneSize, drain_app,
    poll_app,
};

const SCENE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

const SCENE_USAGE: wgpu::TextureUsages = wgpu::TextureUsages::RENDER_ATTACHMENT
    .union(wgpu::TextureUsages::TEXTURE_BINDING)
    .union(wgpu::TextureUsages::COPY_SRC)
    .union(wgpu::TextureUsages::COPY_DST);
