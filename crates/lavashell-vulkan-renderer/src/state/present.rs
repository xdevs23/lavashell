// SPDX-License-Identifier: GPL-3.0-only
use super::targets::AttachedSurface;
use super::{Drawn, FrameError};
use crate::vulkan::{CompletedScene, DrawnFrame, VulkanRenderer};
use lavashell_core::prelude::{PixelSize, Presented, Rect};
use lavashell_core::surface::SurfaceId;
use lavashell_platform::{FrameOutcome, PresentFrame, SurfaceControl};
use lavashell_renderer::{FrameDevice, SceneImage, SurfaceFrame};

#[derive(Clone, Copy)]
pub(super) struct FrameInputs<'a> {
    pub surface: SurfaceId,
    pub frame: PresentFrame<'a>,
    pub shown: Presented<'a>,
    pub physical: PixelSize,
    pub damage: Option<Rect>,
    pub scene: Option<&'a CompletedScene<SceneImage>>,
}

impl<C: SurfaceControl> AttachedSurface<C> {
    pub(super) fn prepare(&mut self, frame: &PresentFrame) -> Result<(), FrameError<C::Error>> {
        self.control.prepare(frame).map_err(FrameError::Control)
    }

    pub(super) fn present(
        &mut self,
        device: FrameDevice<'_, VulkanRenderer>,
        inputs: FrameInputs<'_>,
    ) -> Result<Drawn, FrameError<C::Error>> {
        let FrameDevice {
            gpu,
            painter,
            streams,
        } = device;
        let FrameInputs {
            surface,
            frame,
            shown,
            physical,
            damage,
            scene,
        } = inputs;
        let paint_frame = |canvas: &skia_safe::Canvas| -> Result<(), FrameError<C::Error>> {
            self.control.before_present(&frame);
            painter
                .paint_timed_surface(
                    canvas,
                    SurfaceFrame {
                        surface,
                        tree: shown,
                        dp_to_output: frame.scaled_viewport.dp_to_output(),
                        damage,
                        clear: skia_safe::Color::TRANSPARENT,
                        scene: scene.map(|scene| &scene.content),
                        shape: self.outline.as_ref(),
                        streams,
                    },
                    frame.time.sequence,
                )
                .map_err(FrameError::Paint)?;
            (self.control)
                .painted(&frame, canvas)
                .map_err(FrameError::Control)
        };
        let outcome = gpu
            .draw_timed_in(&mut self.target, physical, frame.present_at, paint_frame)
            .map_err(FrameError::Draw)
            .and_then(|drawn| match drawn {
                DrawnFrame::Skipped(skipped) => Ok(skipped.outcome()),
                DrawnFrame::Drawn { outcome, painted } => painted.map(|()| outcome),
            })
            .inspect_err(|_| self.damage.invalidate())?;
        self.control.after_present(outcome);
        self.damage.forget_unless_shown(outcome);
        let gpu_time = (outcome == FrameOutcome::Shown)
            .then(|| painter.take_gpu_frame_time(surface))
            .flatten();
        Ok(Drawn {
            outcome,
            stats: painter.render_stats(),
            gpu: gpu_time,
            output: self.target.output(),
            scene_generation: scene.map(|scene| scene.generation),
        })
    }
}
