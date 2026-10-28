// SPDX-License-Identifier: GPL-3.0-only
//! Lavashell overlays and scene export for a Bevy app, both running on the Vulkan device used by Bevy.

mod error;
mod overlay_gpu;
pub mod scene;
#[cfg(test)]
mod test_error;
mod texture_types;
mod timings;
mod vulkan_host;

pub use error::Error;
pub use overlay_gpu::LavashellOverlayPlugin;
pub use timings::{OverlayImageTrace, OverlayTiming};

use bevy::{prelude::*, render::MainWorld, render::extract_component::ExtractComponent};
use lavashell_core::error::message_chain;
use lavashell_core::prelude::*;
use lavashell_core::view::FrameError;
use std::{
    sync::{Arc, Mutex, MutexGuard, PoisonError},
    time::Duration,
};

/// Marks a camera whose resolved image receives the Lavashell overlay.
#[derive(Component, Clone, ExtractComponent)]
pub struct OverlayCamera;

/// The scene tree that the overlay paints, shared with the render world.
#[derive(Resource, Default)]
pub struct OverlayFrame {
    tree: Arc<Mutex<SceneTree>>,
    pub sequence: u64,
    /// Set by `show_frame_at`. An app that renders without `show_frame_at` calls `View::record_presentation` with the
    /// presentation time of the render instead, so that input events hit the content that the overlay showed then.
    pub shown: Option<OverlayShown>,
}

impl OverlayFrame {
    /// Shows a frame of `view` at the size of the overlay in dp and the density of the surface, and logs each
    /// component that failed in it as an error. After an error, the tree is empty and the next frame shows the whole
    /// scene.
    ///
    /// # Errors
    /// Fails as `View::show_frame_at` does.
    pub fn show_frame_at(
        &mut self,
        view: &mut View,
        viewport: Viewport,
        text: &mut impl TextMeasurer,
        timing: FrameTime,
    ) -> Result<(), FrameError> {
        let failures = view.show_frame_at(&mut lock(&self.tree), viewport, text, timing)?;
        for failure in failures {
            log::error!("{}", message_chain(failure));
        }
        self.shown = Some(OverlayShown {
            density: viewport.density,
            presentation: timing.presentation,
        });
        Ok(())
    }
}

#[derive(Clone, Copy, Debug)]
pub struct OverlayShown {
    pub density: Density,
    /// The presentation time of the shown frame, measured from the same epoch as the times passed to
    /// `OverlayFrame::show_frame_at`.
    pub presentation: Duration,
}

#[derive(Resource)]
struct ExtractedOverlay {
    sequence: u64,
    shown: Option<OverlayShown>,
    tree: Arc<Mutex<SceneTree>>,
}

fn extract_overlay(main: Res<MainWorld>, mut commands: Commands) {
    let Some(frame) = main.into_inner().get_resource::<OverlayFrame>() else {
        return;
    };
    commands.insert_resource(ExtractedOverlay {
        sequence: frame.sequence,
        shown: frame.shown,
        tree: frame.tree.clone(),
    });
}

/// Locks `tree`, also after a panic poisoned the lock. A panic while the tree was locked leaves the tree intact,
/// because painting only reads the tree.
fn lock(tree: &Mutex<SceneTree>) -> MutexGuard<'_, SceneTree> {
    tree.lock().unwrap_or_else(PoisonError::into_inner)
}

fn prepare_overlay_targets(
    mut targets: Query<&mut bevy::camera::CameraMainTextureUsages, With<OverlayCamera>>,
) {
    for mut target in &mut targets {
        let required = wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST;
        if !target.0.contains(required) {
            target.0 |= required;
        }
    }
}
