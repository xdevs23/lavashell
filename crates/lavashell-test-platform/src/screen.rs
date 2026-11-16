// SPDX-License-Identifier: GPL-3.0-only
use crate::capture::ShownFrame;
use crate::{RecordedFrame, RecordedFrames};
use lavashell_actor::Receiver;
use lavashell_core::frame::FrameTime;
use lavashell_core::surface::SurfaceId;
use std::collections::BTreeMap;
use std::mem;

pub(crate) struct Screen {
    shown: Receiver<ShownFrame>,
    newest: BTreeMap<SurfaceId, ShownFrame>,
    unpresented: Vec<(SurfaceId, FrameTime)>,
    recorded: Vec<RecordedFrame>,
}

impl Screen {
    pub(crate) fn new(shown: Receiver<ShownFrame>) -> Self {
        Self {
            shown,
            newest: BTreeMap::new(),
            unpresented: Vec::new(),
            recorded: Vec::new(),
        }
    }

    pub(crate) fn take_shown(&mut self) {
        for frame in self.shown.take_arrived().into_iter().flatten() {
            self.unpresented.push((frame.surface, frame.time));
            let _replaced_frame_is_not_needed = self.newest.insert(frame.surface, frame);
        }
    }

    pub(crate) fn take_unpresented(&mut self) -> Vec<(SurfaceId, FrameTime)> {
        mem::take(&mut self.unpresented)
    }

    pub(crate) fn record(&mut self) {
        self.recorded
            .extend(self.newest.values().filter_map(|frame| {
                Some(RecordedFrame {
                    surface: frame.surface,
                    time: frame.time,
                    png: frame.png.clone()?,
                })
            }));
    }

    pub(crate) fn newest_frames(&self) -> Vec<(SurfaceId, u64)> {
        (self.newest.values())
            .map(|frame| (frame.surface, frame.time.sequence))
            .collect()
    }

    pub(crate) fn remove(&mut self, surface: SurfaceId) {
        let _removed_frame_is_not_needed = self.newest.remove(&surface);
        self.unpresented.retain(|&(shown, _)| shown != surface);
    }

    pub(crate) fn into_frames(self) -> RecordedFrames {
        RecordedFrames::new(self.recorded)
    }
}
