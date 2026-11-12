// SPDX-License-Identifier: GPL-3.0-only
use crate::{Error, RecordedFrame};
use lavashell_actor::{Effect, Merge, Sender};
use lavashell_core::frame::FrameTime;
use lavashell_core::surface::{SurfaceId, SurfaceState};
use lavashell_renderer::{FrameOutcome, PresentFrame, Snapshot, SurfaceControl};

/// The control for a surface on the test platform, which sends each frame shown by the surface to the script.
pub struct FrameCapture {
    surface: SurfaceId,
    mode: CaptureMode,
    shown: Sender<ShownFrame>,
    /// The frame painted last, until the renderer reports whether it was shown.
    painted: Option<ShownFrame>,
}

/// Whether the control for a surface encodes the pixels of each shown frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureMode {
    WithoutPixels,
    WithPixels,
}

/// A frame shown by a surface, with its pixels encoded as PNG when they were captured.
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

impl ShownFrame {
    /// The frame as a recording contains it, when its pixels were captured.
    pub(crate) fn recorded(&self) -> Option<RecordedFrame> {
        Some(RecordedFrame {
            surface: self.surface,
            time: self.time,
            png: self.png.clone()?,
        })
    }
}

impl CaptureMode {
    fn png(self, snapshot: &Snapshot<'_>) -> Result<Option<Vec<u8>>, lavashell_renderer::Error> {
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

    fn prepare(&mut self, _: &PresentFrame) -> Result<(), Error> {
        Ok(())
    }

    fn before_present(&mut self, _: &PresentFrame) {}

    fn painted(&mut self, frame: &PresentFrame, snapshot: Snapshot<'_>) -> Result<(), Error> {
        let surface = self.surface;
        let png =
            (self.mode.png(&snapshot)).map_err(|source| Error::EncodeFrame { surface, source })?;
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

    fn commit_state(&mut self, _: &SurfaceState) -> Result<(), Error> {
        Ok(())
    }
}

impl Merge for ShownFrame {
    type Key = ();

    fn effect(&self) -> Effect<()> {
        Effect::BuildsOn
    }
}
