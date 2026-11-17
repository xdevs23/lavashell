// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::frame::FrameTime;
use lavashell_core::surface::SurfaceId;
use std::collections::BTreeMap;

pub struct RecordedFrames {
    frames: Vec<RecordedFrame>,
    app_ids: BTreeMap<SurfaceId, String>,
}

#[derive(Clone)]
pub struct RecordedFrame {
    pub surface: SurfaceId,
    pub time: FrameTime,
    pub png: Vec<u8>,
}

impl RecordedFrames {
    pub(crate) fn new(frames: Vec<RecordedFrame>, app_ids: BTreeMap<SurfaceId, String>) -> Self {
        Self { frames, app_ids }
    }

    #[must_use]
    pub fn frames(&self) -> &[RecordedFrame] {
        &self.frames
    }

    pub fn of_window<'a>(&'a self, app_id: &'a str) -> impl Iterator<Item = &'a RecordedFrame> {
        (self.frames.iter()).filter(move |frame| {
            self.app_ids
                .get(&frame.surface)
                .is_some_and(|id| id == app_id)
        })
    }

    #[must_use]
    pub fn into_frames(self) -> Vec<RecordedFrame> {
        self.frames
    }
}
