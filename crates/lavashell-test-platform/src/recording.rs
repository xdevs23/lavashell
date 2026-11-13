// SPDX-License-Identifier: GPL-3.0-only
use crate::Error;
use crate::capture::ShownFrame;
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

impl RecordedFrame {
    /// `frame` as a recording contains it, with its pixels encoded as PNG at most once. `None` when the control for its
    /// surface did not capture its pixels.
    ///
    /// # Errors
    ///
    /// Fails with [`Error::EncodeFrame`] when the pixels cannot be encoded.
    pub(crate) fn encode(frame: &ShownFrame) -> Result<Option<Self>, Error> {
        let Some(pixels) = &frame.pixels else {
            return Ok(None);
        };
        let surface = frame.surface;
        let png = if let Some(png) = frame.png.get() {
            png
        } else {
            let encoded =
                (pixels.png()).map_err(|source| Error::EncodeFrame { surface, source })?;
            frame.png.get_or_init(|| encoded)
        };
        Ok(Some(Self {
            surface,
            time: frame.time,
            png: png.clone(),
        }))
    }
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
