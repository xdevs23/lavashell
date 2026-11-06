// SPDX-License-Identifier: GPL-3.0-only
//! Lavashell overlays and scene export for a Bevy app, both running on the Vulkan device used by Bevy.

mod error;
mod overlay_gpu;
pub mod scene;
#[cfg(test)]
mod test_error;
#[cfg(test)]
mod tests;
mod texture_types;
mod timings;
mod vulkan_host;

pub use error::{Error, ShowFrameError};
pub use overlay_gpu::LavashellOverlayPlugin;
pub use timings::{OverlayImageTrace, OverlayTiming};

use bevy::{prelude::*, render::MainWorld, render::extract_component::ExtractComponent};
use lavashell_actor::{Closed, Latest};
use lavashell_core::error::message_chain;
use lavashell_core::frame::FrameTime;
use lavashell_core::geometry::Viewport;
use lavashell_core::scene::tree::ApplyError;
use lavashell_core::surface::SurfaceId;
use lavashell_core::units::Density;
use lavashell_core::view::{FrameError, TextMeasurer, View};
use lavashell_runtime::{BuilderReport, BuiltSurface, FrameSequence, ScenePipeline};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

/// The only surface in the scene pipeline of the overlay.
const OVERLAY_SURFACE: SurfaceId = SurfaceId(0);

/// Marks a camera whose resolved image receives the Lavashell overlay.
#[derive(Component, Clone, ExtractComponent)]
pub struct OverlayCamera;

/// The scene painted by the overlay. The choreographer and the scene builder run on separate threads and build the
/// frames of the view into the scene. The render world paints the newest built scene.
#[derive(Resource)]
pub struct OverlayFrame {
    scene: ScenePipeline,
    /// The error of a frame that did not fit the built scene, until the next frame sends the whole scene.
    restart: Arc<Latest<ApplyError>>,
    /// The frames sent since the newest frame in the built scene, that frame included.
    sent: BTreeMap<FrameSequence, SentFrame>,
    /// The number that `show_frame_at` attaches to the frame it sends. The timing record of each drawn frame contains
    /// the number of that frame.
    pub sequence: u64,
}

/// The values sent with a frame. The render world paints the scene built from that frame with them.
#[derive(Clone, Copy)]
struct SentFrame {
    sequence: u64,
    density: Density,
    /// Measured from the same epoch as the times passed to `OverlayFrame::show_frame_at`.
    presentation: Duration,
}

impl OverlayFrame {
    fn start() -> Result<Self, Error> {
        let restart = Arc::new(Latest::new());
        let published = Arc::clone(&restart);
        let observer = move |report: BuilderReport| -> Result<(), Closed> {
            if let BuilderReport::Restart { error, .. } = report {
                published.publish(error);
            }
            Ok(())
        };
        Ok(Self {
            scene: ScenePipeline::start(Box::new(observer)).map_err(Error::StartScene)?,
            restart,
            sent: BTreeMap::new(),
            sequence: 0,
        })
    }

    /// Whether `show_frame_at` has a frame to send: the view needs one, or the scene builder asked for the whole
    /// scene. An app that shows a frame only when it needs one checks this on each update.
    #[must_use]
    pub fn needs_frame(&self, view: &View) -> bool {
        view.needs_frame() || self.restart.read().is_some()
    }

    /// Sends a frame of `view` at the size of the overlay in dp and the density of the surface to the choreographer,
    /// and logs each component that failed in it as an error. After an error, the overlay shows the last frame that
    /// was built. The next frame sends the whole scene.
    ///
    /// # Errors
    /// Fails as `View::frame_into_at` does. Also fails with the error of the choreographer or the scene builder after
    /// either has stopped.
    pub fn show_frame_at(
        &mut self,
        view: &mut View,
        viewport: Viewport,
        text: &mut impl TextMeasurer,
        timing: FrameTime,
    ) -> Result<(), ShowFrameError> {
        if let Some(error) = self.restart.take() {
            self.scene.restart_view(OVERLAY_SURFACE, view, &error);
        }
        let end = FrameSequence(self.sent.last_key_value().map_or(0, |(last, _)| last.0 + 1));
        let mut sink = self.scene.sink(OVERLAY_SURFACE, Some(end));
        let failures = match view.frame_into_at(viewport, text, timing, &mut sink) {
            Ok(failures) => failures,
            Err(FrameError::Mirror(error)) => return Err(ShowFrameError::Mirror(error)),
            Err(FrameError::Sink(Closed)) => {
                return Err(match self.scene.stop() {
                    Err(failure) => ShowFrameError::Scene(failure),
                    Ok(()) => ShowFrameError::SceneStopped,
                });
            }
        };
        for failure in failures {
            log::error!("{}", message_chain(failure));
        }
        if let Some(built) = self.scene.built(OVERLAY_SURFACE).sequence {
            self.sent = self.sent.split_off(&built);
        }
        let sent = SentFrame {
            sequence: self.sequence,
            density: viewport.density,
            presentation: timing.presentation,
        };
        let _replaced = self.sent.insert(end, sent);
        Ok(())
    }

    /// The newest built scene, with the frame that it ends with.
    fn extracted(&self) -> ExtractedOverlay {
        let built = self.scene.built(OVERLAY_SURFACE);
        let sent = (built.sequence).and_then(|sequence| self.sent.get(&sequence).copied());
        ExtractedOverlay { sent, built }
    }
}

/// Starts the scene pipeline of the overlay in the main world. Bevy passes a start failure to the error handler of the
/// app.
struct StartOverlayScene;

impl Command<Result<(), Error>> for StartOverlayScene {
    fn apply(self, world: &mut World) -> Result<(), Error> {
        world.insert_resource(OverlayFrame::start()?);
        Ok(())
    }
}

#[derive(Resource)]
struct ExtractedOverlay {
    /// The frame that the built scene ends with, or `None` before the scene builder built a frame.
    sent: Option<SentFrame>,
    built: Arc<BuiltSurface>,
}

fn extract_overlay(main: Res<MainWorld>, mut commands: Commands) {
    let Some(frame) = main.into_inner().get_resource::<OverlayFrame>() else {
        return;
    };
    commands.insert_resource(frame.extracted());
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
