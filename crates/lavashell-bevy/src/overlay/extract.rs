// SPDX-License-Identifier: GPL-3.0-only

use super::frame::{OverlayFrame, SentFrame};
use bevy::{prelude::*, render::MainWorld, render::extract_component::ExtractComponent};
use lavashell_core::surface::SurfaceId;
use lavashell_runtime::surface_slots::BuiltSurface;
use std::sync::Arc;

const OVERLAY_TARGET_USAGE: wgpu::TextureUsages = wgpu::TextureUsages::RENDER_ATTACHMENT
    .union(wgpu::TextureUsages::TEXTURE_BINDING)
    .union(wgpu::TextureUsages::COPY_SRC)
    .union(wgpu::TextureUsages::COPY_DST);

#[derive(Component, Clone, ExtractComponent)]
pub struct OverlayCamera;

#[derive(Resource)]
pub(super) struct ExtractedOverlay {
    pub(super) surface: SurfaceId,
    pub(super) sent: Option<SentFrame>,
    pub(super) built: Arc<BuiltSurface>,
}

pub(super) fn extract_overlay(main: Res<MainWorld>, mut commands: Commands) {
    let Some(frame) = main.into_inner().get_resource::<OverlayFrame>() else {
        return;
    };
    commands.insert_resource(frame.extracted());
}

pub(super) fn prepare_overlay_targets(
    mut targets: Query<&mut bevy::camera::CameraMainTextureUsages, With<OverlayCamera>>,
) {
    for mut target in &mut targets {
        if !target.0.contains(OVERLAY_TARGET_USAGE) {
            target.0 |= OVERLAY_TARGET_USAGE;
        }
    }
}
