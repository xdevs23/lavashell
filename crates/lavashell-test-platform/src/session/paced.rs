// SPDX-License-Identifier: GPL-3.0-only
use super::{Session, Settled, record};
use crate::capture::ShownFrame;
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

/// The sequence of the frame recorded last for each window.
#[derive(Default)]
struct LastRecorded(BTreeMap<SurfaceId, u64>);

impl Session {
    /// Plays the run at the pace of the steps that publish to `pace`, as
    /// [`HostScript::record_paced`](crate::HostScript::record_paced) describes. Frames that keep arriving at an
    /// unchanged time without a step asking for them fail the play with [`Error::NoRest`].
    pub(super) fn record_paced(&mut self, pace: &Pace) -> Result<Settled, Error> {
        let mut last_recorded = LastRecorded::default();
        let settled = self.play_paced(pace, &mut last_recorded)?;
        let shown = (self.screen.values()).chain(&self.closed);
        let new = shown.filter(|frame| last_recorded.is_new(frame));
        record(&mut self.recorded, new)?;
        Ok(settled)
    }

    /// Plays the run at the pace of the steps until the step ends. Once the actors have nothing left to do after the
    /// steps asked `pace` to record, records the newest frame of each window that is new to `last_recorded`.
    fn play_paced(
        &mut self,
        pace: &Pace,
        last_recorded: &mut LastRecorded,
    ) -> Result<Settled, Error> {
        let mut unasked = Unasked::None;
        loop {
            let Settled::Waiting(deadline) = self.settle()? else {
                return Ok(Settled::Ended);
            };
            if pace.take_record_request() {
                let new = (self.screen.values()).filter(|frame| last_recorded.is_new(frame));
                record(&mut self.recorded, new)?;
            }
            let input = pace.take_input();
            if !input.is_empty() {
                for event in input {
                    self.links.reports.report(event);
                }
                continue;
            }
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

impl LastRecorded {
    /// Whether the sequence of `frame` differs from the one recorded last for its window. Stores the sequence of
    /// `frame` as the one recorded last.
    fn is_new(&mut self, frame: &ShownFrame) -> bool {
        self.0.insert(frame.surface, frame.time.sequence) != Some(frame.time.sequence)
    }
}
