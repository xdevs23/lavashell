// SPDX-License-Identifier: GPL-3.0-only
use super::targets::AttachedSurface;
use super::{Drawn, FrameError, PresentFrame, Snapshot, SurfaceControl};
use crate::stream::FrameDevice;
use crate::vulkan::{CompletedScene, SceneImage, VulkanRenderer};
use crate::{FrameOutcome, SurfaceFrame};
use lavashell_core::prelude::{Affine, Presented, Rect};
use lavashell_core::surface::SurfaceId;

/// The surface that one frame is drawn into, with the values that the frame paints.
pub(super) struct FrameInputs<'a, C> {
    pub surface: SurfaceId,
    pub attached: &'a mut AttachedSurface<C>,
    pub frame: PresentFrame<'a>,
    pub dp_to_output: Affine,
    pub shown: Presented<'a>,
    /// The size of the frame in physical pixels.
    pub physical: (u32, u32),
    pub damage: Option<Rect>,
    pub scene: Option<&'a CompletedScene<SceneImage>>,
}

/// Paints the frame of `inputs` and presents it to the target of its surface.
pub(super) fn present_frame<C: SurfaceControl>(
    device: FrameDevice<'_, VulkanRenderer>,
    inputs: FrameInputs<'_, C>,
) -> Result<Drawn, FrameError<C::Error>> {
    let FrameDevice {
        gpu,
        painter,
        streams,
    } = device;
    let FrameInputs {
        surface,
        attached,
        frame,
        dp_to_output,
        shown,
        physical,
        damage,
        scene,
    } = inputs;
    let mut painted = Ok(());
    let mut control_result = Ok(());
    let paint_frame = |canvas: &skia_safe::Canvas| {
        attached.control.before_present(&frame);
        painted = painter.paint_surface(
            canvas,
            SurfaceFrame {
                tree: shown,
                dp_to_output,
                damage,
                clear: skia_safe::Color::TRANSPARENT,
                scene: scene.map(|scene| &scene.content),
                shape: attached.outline.as_ref(),
                streams,
            },
        );
        if painted.is_ok() {
            control_result = attached.control.painted(&frame, Snapshot::new(canvas));
        }
    };
    let outcome = gpu
        .draw_timed_in(
            surface,
            &mut attached.target,
            physical,
            frame.time.sequence,
            frame.present_at,
            paint_frame,
        )
        .map_err(FrameError::Draw)
        .and_then(|outcome| painted.map(|()| outcome).map_err(FrameError::Paint))
        .and_then(|outcome| {
            control_result
                .map(|()| outcome)
                .map_err(FrameError::Control)
        })
        .inspect_err(|_| attached.damage.invalidate())?;
    attached.control.after_present(outcome);
    attached.damage.forget_unless_shown(outcome);
    let gpu_time = (outcome == FrameOutcome::Shown)
        .then(|| painter.take_gpu_frame_time(surface))
        .flatten();
    Ok(Drawn {
        outcome,
        stats: painter.render_stats(),
        gpu: gpu_time,
        output: attached.target.output(),
        scene: scene.map(|scene| scene.generation),
    })
}
