// SPDX-License-Identifier: GPL-3.0-only
use crate::SurfaceRecords;
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

/// The panels made by a factory for each output, each described by a `P` with a view. The panels of an output
/// close when the host removes the output.
pub struct PanelsPerOutput<P> {
    factory: Factory<P>,
    groups: BTreeMap<OutputId, Group>,
}

impl<P> PanelsPerOutput<P> {
    /// Runs `factory` once for each output, with the geometry of the output. The geometry passed to the factory
    /// changes when the geometry of the output changes.
    #[must_use]
    pub fn new(factory: impl FnMut(State<MonitorInfo>) -> Vec<(P, View)> + 'static) -> Self {
        Self {
            factory: Box::new(factory),
            groups: BTreeMap::new(),
        }
    }

    /// Applies `info`, the geometry of `output`, or `None` while the geometry is unknown. An output without panels
    /// gets them from the factory once its geometry is known, and `open` opens the surface of each. The surfaces
    /// are recorded for the output only after every surface opened. The panels of an output that has them get the
    /// new geometry.
    ///
    /// # Errors
    ///
    /// Fails with the error of `open` when a surface does not open.
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

    /// Closes the panels of `output`, which the host removed, and returns the requests that follow.
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
