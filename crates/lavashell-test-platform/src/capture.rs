// SPDX-License-Identifier: GPL-3.0-only
use crate::Error;
use lavashell_actor::{Effect, Merge, Sender};
use lavashell_core::frame::FrameTime;
use lavashell_core::surface::{SurfaceId, SurfaceState};
use lavashell_renderer::{FrameOutcome, Pixels, PresentFrame, Snapshot, SurfaceControl};
use std::cell::OnceCell;

/// The control for a surface on the test platform, which sends each frame shown by the surface to the script.
pub struct FrameCapture {
    surface: SurfaceId,
    mode: CaptureMode,
    shown: Sender<ShownFrame>,
    /// The frame painted last, until the renderer reports whether it was shown.
    painted: Option<ShownFrame>,
}

/// Whether the control for a surface reads the pixels of each shown frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureMode {
    WithoutPixels,
    WithPixels,
}

/// A frame shown by a surface, with its pixels when they were captured.
pub(crate) struct ShownFrame {
    pub(crate) surface: SurfaceId,
    pub(crate) time: FrameTime,
    pub(crate) pixels: Option<Pixels>,
    /// The pixels encoded as PNG, once a recording contains the frame.
    pub(crate) png: OnceCell<Vec<u8>>,
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
    fn pixels(self, snapshot: &Snapshot<'_>) -> Result<Option<Pixels>, lavashell_renderer::Error> {
        match self {
            Self::WithoutPixels => Ok(None),
            Self::WithPixels => snapshot.pixels().map(Some),
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
        let pixels =
            (self.mode.pixels(&snapshot)).map_err(|source| Error::ReadFrame { surface, source })?;
        self.painted = Some(ShownFrame {
            surface,
            time: frame.time,
            pixels,
            png: OnceCell::new(),
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
