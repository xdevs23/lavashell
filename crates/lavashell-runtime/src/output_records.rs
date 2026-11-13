// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::output::MonitorInfo;
use lavashell_core::surface::SurfaceId;
use lavashell_platform::{OutputEvent, OutputId};
use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

#[derive(Default)]
struct Record {
    info: Option<MonitorInfo>,
    refresh: Option<Duration>,
    entered: BTreeSet<SurfaceId>,
}

/// The record of every output reported by the host, with the surfaces shown on it.
#[derive(Default)]
pub(crate) struct OutputRecords {
    records: BTreeMap<OutputId, Record>,
}

impl OutputRecords {
    /// Applies `event` reported by the host. `Added`, `Updated`, `Refresh` and `Entered` create the record of an
    /// output that has none, so a surface that enters an output before the geometry of the output arrives is still
    /// recorded. `Removed` deletes the record with the surfaces shown on the output. `Left` for an output without a
    /// record changes nothing.
    pub fn handle(&mut self, event: OutputEvent) {
        match event {
            OutputEvent::Added(output) => {
                let _record = self.record_mut(output);
            }
            OutputEvent::Updated(output, info) => self.record_mut(output).info = Some(info),
            OutputEvent::Removed(output) => {
                let _removed_record = self.records.remove(&output);
            }
            OutputEvent::Refresh(output, refresh) => self.record_mut(output).refresh = refresh,
            OutputEvent::Entered(output, surface) => {
                let _newly_entered = self.record_mut(output).entered.insert(surface);
            }
            OutputEvent::Left(output, surface) => {
                if let Some(record) = self.records.get_mut(&output) {
                    let _was_entered = record.entered.remove(&surface);
                }
            }
        }
    }

    /// Removes `surface` from every output. The host does not send a leave event when a surface is destroyed.
    pub fn forget(&mut self, surface: SurfaceId) {
        for record in self.records.values_mut() {
            let _was_entered = record.entered.remove(&surface);
        }
    }

    /// The geometry of `output`, or `None` until the host reports it.
    #[must_use]
    pub fn info(&self, output: OutputId) -> Option<&MonitorInfo> {
        self.records.get(&output)?.info.as_ref()
    }

    /// The time between two refreshes of each output that shows `surface` and reports one, in the order of the
    /// output ids.
    pub fn refreshes(&self, surface: SurfaceId) -> impl Iterator<Item = Duration> + '_ {
        (self.records.values())
            .filter(move |record| record.entered.contains(&surface))
            .filter_map(|record| record.refresh)
    }

    fn record_mut(&mut self, output: OutputId) -> &mut Record {
        self.records.entry(output).or_default()
    }
}

#[cfg(test)]
mod tests;
