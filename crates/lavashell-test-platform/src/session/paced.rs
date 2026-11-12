// SPDX-License-Identifier: GPL-3.0-only
use super::{Session, Settled};
use crate::{Error, Pace, StepsPace, is_at_rest};
use lavashell_actor::Clock;
use lavashell_core::surface::SurfaceId;
use std::collections::BTreeMap;
use std::time::Duration;

/// The presents in a row at an unchanged time on the clock that a step did not ask for. A frame can change the state of
/// the app, such as through the output an observer receives for it, and so make one more frame at the same time. A
/// further frame after that one means that the app makes a frame for each present.
#[derive(Clone, Copy)]
enum Unasked {
    None,
    Once,
    Twice,
}

impl Session {
    /// Plays the run at the pace of the steps that publish to `pace`, as
    /// [`HostScript::record_paced`](crate::HostScript::record_paced) describes. Frames that keep arriving at an
    /// unchanged time without a step asking for them fail the play with [`Error::NoRest`].
    pub(super) fn record_paced(&mut self, pace: &Pace) -> Result<Settled, Error> {
        let mut recorded = BTreeMap::new();
        let mut unasked = Unasked::None;
        loop {
            let Settled::Waiting(deadline) = self.settle()? else {
                return Ok(Settled::Ended);
            };
            self.record_new(&mut recorded);
            let now = self.clock.now();
            let shown = !self.unpresented.is_empty();
            let (due, every) = match pace.read() {
                Some(StepsPace::Ended) => return Ok(Settled::Waiting(deadline)),
                Some(StepsPace::Queued) if shown => (Some(now), None),
                Some(StepsPace::Waiting { due, every }) => (due.filter(|&due| due > now), every),
                Some(StepsPace::Queued) | None => (None, None),
            };
            let waited = deadline.into_iter().chain(due).min();
            if is_at_rest(shown, waited) {
                return Ok(Settled::Waiting(None));
            }
            let sample = every.map(|every| now.saturating_add(every.get()));
            let next = waited.into_iter().chain(sample).min();
            unasked = if next.is_some() {
                Unasked::None
            } else {
                unasked.again(now)?
            };
            if let Settled::Ended = self.advance(next.unwrap_or(now))? {
                return Ok(Settled::Ended);
            }
        }
    }

    /// Records the newest frame of each window whose sequence differs from the one in `recorded`, and stores its
    /// sequence there.
    fn record_new(&mut self, recorded: &mut BTreeMap<SurfaceId, u64>) {
        for frame in self.screen.values() {
            if recorded.insert(frame.surface, frame.time.sequence) != Some(frame.time.sequence) {
                self.recorded.extend(frame.recorded());
            }
        }
    }
}

impl Unasked {
    /// The count after one more present at `time` that no step asked for.
    fn again(self, time: Duration) -> Result<Self, Error> {
        match self {
            Self::None => Ok(Self::Once),
            Self::Once => Ok(Self::Twice),
            Self::Twice => Err(Error::NoRest { time }),
        }
    }
}
