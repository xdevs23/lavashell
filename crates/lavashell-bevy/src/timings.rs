// SPDX-License-Identifier: GPL-3.0-only
use bevy::render::render_resource::{TextureId, TextureViewId};
use std::sync::mpsc::Sender;
use std::time::Duration;

/// The sequence number and timings of one frame that the Lavashell overlay drew into a Bevy view and submitted.
/// `submit` is the time of the Skia draw and its submission. `host_flush` is the time of the submission of the commands
/// that Bevy recorded before the draw.
#[derive(Clone, Copy, Debug)]
pub struct OverlayTiming {
    pub sequence: u64,
    pub submit: Duration,
    pub host_flush: Duration,
    pub image: OverlayImageTrace,
}

/// The wgpu IDs of the main texture and views of the Bevy view, read on the CPU before and after the overlay draws into
/// the main texture. The IDs show which resources the draw used on the CPU side. They do not show that the GPU has
/// finished the draw.
#[derive(Clone, Copy, Debug)]
pub struct OverlayImageTrace {
    pub texture_before: TextureId,
    pub texture_after: TextureId,
    pub view_before: TextureViewId,
    pub view_after: TextureViewId,
    pub output_view: TextureViewId,
    pub size: (u32, u32),
    pub format: wgpu::TextureFormat,
    pub host_command_buffers: usize,
    pub overlay_submission: u64,
}

/// The channel to which the overlay sends an [`OverlayTiming`] for every frame that it submits. The receiver counts and
/// stores the timings, so file I/O runs outside the render node.
pub(crate) struct TimingRecords(Option<Sender<OverlayTiming>>);

impl TimingRecords {
    pub fn new(records: Sender<OverlayTiming>) -> Self {
        Self(Some(records))
    }

    pub fn send(&mut self, timing: OverlayTiming) {
        if let Some(records) = &self.0
            && records.send(timing).is_err()
        {
            // A send fails for every later timing once the receiver is gone. Dropping the sender skips those sends.
            self.0 = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_error::TestError;
    use std::sync::mpsc;

    fn timing(sequence: u64) -> OverlayTiming {
        OverlayTiming {
            sequence,
            submit: Duration::ZERO,
            host_flush: Duration::ZERO,
            image: OverlayImageTrace {
                texture_before: TextureId::new(),
                texture_after: TextureId::new(),
                view_before: TextureViewId::new(),
                view_after: TextureViewId::new(),
                output_view: TextureViewId::new(),
                size: (1, 1),
                format: wgpu::TextureFormat::Rgba8Unorm,
                host_command_buffers: 0,
                overlay_submission: sequence,
            },
        }
    }

    #[test]
    fn records_go_to_the_receiver_until_it_is_gone() -> Result<(), TestError> {
        let (sender, receiver) = mpsc::channel();
        let mut records = TimingRecords::new(sender);
        records.send(timing(1));
        assert_eq!(receiver.recv()?.sequence, 1);
        drop(receiver);
        records.send(timing(2));
        assert!(records.0.is_none());
        Ok(())
    }
}
