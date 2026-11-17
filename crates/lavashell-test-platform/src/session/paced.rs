// SPDX-License-Identifier: GPL-3.0-only
use super::{Session, Stop};
use crate::{Error, Pace, PacedLinks, is_at_rest};
use lavashell_actor::Clock;

impl Session {
    pub(super) fn record_paced(&mut self, links: &PacedLinks) -> Result<(), Stop> {
        match self.play_paced(links) {
            Err(Stop::Failed(error)) => Err(Stop::Failed(error)),
            played => {
                self.screen.record_new_and_closed()?;
                played
            }
        }
    }

    fn play_paced(&mut self, links: &PacedLinks) -> Result<(), Stop> {
        let mut presented_unrequested = false;
        loop {
            let deadline = self.settle_until_next_deadline()?;
            if links.record_requests.take() {
                self.screen.record_new()?;
            }
            let input = links.input.take();
            if !input.is_empty() {
                for event in input {
                    self.links.reports.report(event);
                }
                continue;
            }
            let now = self.clock.now();
            let shown = self.screen.has_unpresented();
            let (due, every) = match links.pace.read() {
                Some(Pace::Finished) => return Ok(()),
                Some(Pace::FrameAtSameTime) if shown => (Some(now), None),
                Some(Pace::Wake { at, every }) => (at.filter(|&at| at > now), every),
                Some(Pace::FrameAtSameTime) | None => (None, None),
            };
            let waited = deadline.into_iter().chain(due).min();
            if is_at_rest(shown, waited) {
                return Ok(());
            }
            let sample = every.map(|every| now.saturating_add(every.get()));
            let next = waited.into_iter().chain(sample).min();
            match next {
                Some(time) => {
                    presented_unrequested = false;
                    self.advance(time)?;
                }
                None if presented_unrequested => Err(Error::NoRest { time: now })?,
                None => {
                    presented_unrequested = true;
                    self.advance(now)?;
                }
            }
        }
    }
}
