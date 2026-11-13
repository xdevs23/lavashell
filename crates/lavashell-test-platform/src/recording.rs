// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::frame::FrameTime;
use lavashell_core::surface::SurfaceId;

/// The frames recorded by a script, in the order of their recording.
pub struct RecordedFrames {
    frames: Vec<RecordedFrame>,
}

/// The frame shown by a surface at a time on the run clock, encoded as PNG.
#[derive(Clone)]
pub struct RecordedFrame {
    pub surface: SurfaceId,
    pub time: FrameTime,
    pub png: Vec<u8>,
}

impl RecordedFrames {
    pub(crate) fn new(frames: Vec<RecordedFrame>) -> Self {
        Self { frames }
    }

    #[must_use]
    pub fn frames(&self) -> &[RecordedFrame] {
        &self.frames
    }

    #[must_use]
    pub fn into_frames(self) -> Vec<RecordedFrame> {
        self.frames
    }
}
