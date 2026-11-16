// SPDX-License-Identifier: GPL-3.0-only
use crate::Error;
use lavashell_actor::{Merge, MergeEffect, Sender};
use lavashell_core::frame::FrameTime;
use lavashell_core::surface::SurfaceId;
use lavashell_renderer::{FrameOutcome, PresentFrame, Snapshot, SurfaceControl};

pub struct FrameCapture {
    surface: SurfaceId,
    mode: CaptureMode,
    shown: Sender<ShownFrame>,
    painted: Option<ShownFrame>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureMode {
    WithoutPixels,
    WithPixels,
}

pub(crate) struct ShownFrame {
    pub(crate) surface: SurfaceId,
    pub(crate) time: FrameTime,
    pub(crate) png: Option<Vec<u8>>,
}

impl FrameCapture {
    pub(crate) fn new(surface: SurfaceId, mode: CaptureMode, shown: Sender<ShownFrame>) -> Self {
        Self {
            surface,
            mode,
            shown,
            painted: None,
        }
    }
}

impl CaptureMode {
    fn encode_png_if_wanted(
        self,
        snapshot: &Snapshot<'_>,
    ) -> Result<Option<Vec<u8>>, lavashell_renderer::Error> {
        match self {
            Self::WithoutPixels => Ok(None),
            Self::WithPixels => snapshot.png().map(Some),
        }
    }
}

impl SurfaceControl for FrameCapture {
    type Error = Error;
    type Message = CaptureMode;

    fn receive(&mut self, mode: CaptureMode) {
        self.mode = mode;
    }

    fn painted(&mut self, frame: &PresentFrame, snapshot: Snapshot<'_>) -> Result<(), Error> {
        let surface = self.surface;
        let png = (self.mode.encode_png_if_wanted(&snapshot))
            .map_err(|source| Error::EncodeFrame { surface, source })?;
        self.painted = Some(ShownFrame {
            surface,
            time: frame.time,
            png,
        });
        Ok(())
    }

    fn after_present(&mut self, outcome: FrameOutcome) {
        if let (FrameOutcome::Shown, Some(frame)) = (outcome, self.painted.take()) {
            let _script_gone_after_the_run = self.shown.push(frame);
        }
    }
}

impl Merge for ShownFrame {
    type Key = ();

    fn effect(&self) -> MergeEffect<()> {
        MergeEffect::Appends
    }
}
