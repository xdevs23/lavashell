// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::frame::FrameTime;
use lavashell_core::surface::SurfaceId;

pub struct RecordedFrames {
    frames: Vec<RecordedFrame>,
}

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
