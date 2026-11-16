// SPDX-License-Identifier: GPL-3.0-only
use crate::surface_records::SurfaceRecords;
use lavashell_core::output::MonitorInfo;
use lavashell_core::reactive::State;
use lavashell_core::surface::SurfaceId;
use lavashell_core::view::View;
use lavashell_platform::{OutputId, SurfaceEvent, SurfaceRequest};
use std::collections::BTreeMap;

type Factory<P> = Box<dyn FnMut(State<MonitorInfo>) -> Vec<(P, View)>>;

struct Group {
    info: State<MonitorInfo>,
    surfaces: Vec<SurfaceId>,
}

pub struct PanelsPerOutput<P> {
    factory: Factory<P>,
    groups: BTreeMap<OutputId, Group>,
}

impl<P> PanelsPerOutput<P> {
    #[must_use]
    pub fn new(factory: impl FnMut(State<MonitorInfo>) -> Vec<(P, View)> + 'static) -> Self {
        Self {
            factory: Box::new(factory),
            groups: BTreeMap::new(),
        }
    }

    pub fn output_updated<E>(
        &mut self,
        output: OutputId,
        info: Option<MonitorInfo>,
        mut open: impl FnMut(P, View) -> Result<SurfaceId, E>,
    ) -> Result<(), E> {
        let Some(info) = info else {
            return Ok(());
        };
        if let Some(group) = self.groups.get_mut(&output) {
            group.info.set(info);
            return Ok(());
        }
        let info = State::new(info);
        let surfaces = ((self.factory)(info.clone()).into_iter())
            .map(|(panel, view)| open(panel, view))
            .collect::<Result<_, E>>()?;
        let _replaced_group = self.groups.insert(output, Group { info, surfaces });
        Ok(())
    }

    #[must_use]
    pub fn output_removed(
        &mut self,
        output: OutputId,
        surfaces: &mut SurfaceRecords,
    ) -> Vec<SurfaceRequest> {
        (self.groups.remove(&output).into_iter())
            .flat_map(|group| group.surfaces)
            .filter_map(|surface| surfaces.handle(surface, SurfaceEvent::Closed))
            .collect()
    }
}

#[cfg(test)]
mod tests;
