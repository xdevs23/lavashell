// SPDX-License-Identifier: GPL-3.0-only
use crate::capture::ShownFrame;
use crate::{Error, RecordedFrame};
use lavashell_actor::Receiver;
use lavashell_core::frame::FrameTime;
use lavashell_core::surface::SurfaceId;
use std::collections::BTreeMap;
use std::mem;

pub(crate) struct Screen {
    shown: Receiver<ShownFrame>,
    newest: BTreeMap<SurfaceId, ShownFrame>,
    closed: Vec<ShownFrame>,
    unpresented: Vec<(SurfaceId, FrameTime)>,
    recorded: Vec<RecordedFrame>,
    recorded_sequences: BTreeMap<SurfaceId, u64>,
}

impl Screen {
    pub(crate) fn new(shown: Receiver<ShownFrame>) -> Self {
        Self {
            shown,
            newest: BTreeMap::new(),
            closed: Vec::new(),
            unpresented: Vec::new(),
            recorded: Vec::new(),
            recorded_sequences: BTreeMap::new(),
        }
    }

    pub(crate) fn take_shown(&mut self) {
        for frame in self.shown.take_arrived().into_iter().flatten() {
            self.unpresented.push((frame.surface, frame.time));
            let _replaced_frame_is_not_needed = self.newest.insert(frame.surface, frame);
        }
    }

    pub(crate) fn has_unpresented(&self) -> bool {
        !self.unpresented.is_empty()
    }

    pub(crate) fn take_unpresented(&mut self) -> Vec<(SurfaceId, FrameTime)> {
        mem::take(&mut self.unpresented)
    }

    pub(crate) fn start_step(&mut self) {
        self.closed.clear();
        self.recorded_sequences.clear();
    }

    pub(crate) fn record_new(&mut self) -> Result<(), Error> {
        let sequences = &mut self.recorded_sequences;
        record_unrecorded(&mut self.recorded, sequences, self.newest.values())
    }

    pub(crate) fn record_new_and_closed(&mut self) -> Result<(), Error> {
        let shown = self.newest.values().chain(&self.closed);
        record_unrecorded(&mut self.recorded, &mut self.recorded_sequences, shown)
    }

    pub(crate) fn newest_frames(&self) -> Vec<(SurfaceId, u64)> {
        (self.newest.values())
            .map(|frame| (frame.surface, frame.time.sequence))
            .collect()
    }

    pub(crate) fn remove(&mut self, surface: SurfaceId) {
        self.closed.extend(self.newest.remove(&surface));
        self.unpresented.retain(|&(shown, _)| shown != surface);
    }

    pub(crate) fn into_recorded(self) -> Vec<RecordedFrame> {
        self.recorded
    }
}

fn record_unrecorded<'a>(
    recorded: &mut Vec<RecordedFrame>,
    sequences: &mut BTreeMap<SurfaceId, u64>,
    frames: impl IntoIterator<Item = &'a ShownFrame>,
) -> Result<(), Error> {
    for frame in frames {
        let sequence = frame.time.sequence;
        if sequences.insert(frame.surface, sequence) != Some(sequence) {
            recorded.extend(frame.recorded()?);
        }
    }
    Ok(())
}
