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

#[derive(Default)]
pub(crate) struct OutputRecords {
    records: BTreeMap<OutputId, Record>,
}

impl OutputRecords {
    pub fn handle(&mut self, event: OutputEvent) {
        match event {
            OutputEvent::Added(output) => {
                let _record_is_created_on_first_access = self.record_mut(output);
            }
            OutputEvent::Updated(output, info) => self.record_mut(output).info = Some(info),
            OutputEvent::Removed(output) => {
                let _removed_record = self.records.remove(&output);
            }
            OutputEvent::Refresh(output, refresh) => self.record_mut(output).refresh = refresh,
            OutputEvent::Entered(output, surface) => {
                let _whether_newly_entered_is_not_needed =
                    self.record_mut(output).entered.insert(surface);
            }
            OutputEvent::Left(output, surface) => {
                if let Some(record) = self.records.get_mut(&output) {
                    let _whether_it_was_entered_is_not_needed = record.entered.remove(&surface);
                }
            }
        }
    }

    pub fn forget(&mut self, surface: SurfaceId) {
        for record in self.records.values_mut() {
            let _whether_it_was_entered_is_not_needed = record.entered.remove(&surface);
        }
    }

    #[must_use]
    pub fn info(&self, output: OutputId) -> Option<&MonitorInfo> {
        self.records.get(&output)?.info.as_ref()
    }

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
