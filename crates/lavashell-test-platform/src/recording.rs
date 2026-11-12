// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::frame::FrameTime;
use lavashell_core::surface::SurfaceId;
use std::collections::BTreeMap;

/// The frames recorded by a script, in the order of their recording, with the app id of each window created during
/// the run.
pub struct RecordedFrames {
    frames: Vec<RecordedFrame>,
    app_ids: BTreeMap<SurfaceId, String>,
}

/// The frame shown by a surface at a time on the run clock, encoded as PNG.
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

    /// The frames of the windows with `app_id`, in the order of their recording.
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
