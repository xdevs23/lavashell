// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]

use lavashell_cargo_model::unit_graph::{
    Unit, UnitGraph, UnitIdentity, UnitIndexError, UnitRecord,
};
use std::collections::BTreeSet;

pub fn external_records(graph: &UnitGraph) -> Result<BTreeSet<UnitRecord>, UnitIndexError> {
    graph
        .units
        .iter()
        .filter(|unit| unit.pkg_id.is_external())
        .map(|unit| {
            let dependencies = graph
                .dependencies(unit)
                .map(|dependency| dependency.map(Unit::identity))
                .collect::<Result<_, _>>()?;
            Ok(UnitRecord {
                unit: unit.identity(),
                dependencies,
            })
        })
        .collect()
}

fn engine_units(
    graph: &UnitGraph,
    engine_root: &str,
) -> Result<BTreeSet<UnitIdentity>, UnitIndexError> {
    let mut pending: Vec<usize> = graph
        .units
        .iter()
        .enumerate()
        .filter(|(_, unit)| unit.pkg_id.is_external() && unit.target.name == engine_root)
        .map(|(index, _)| index)
        .collect();
    let mut seen = BTreeSet::new();
    while let Some(index) = pending.pop() {
        if seen.insert(index) {
            pending.extend(
                graph
                    .unit(index)?
                    .dependencies
                    .iter()
                    .map(|edge| edge.index),
            );
        }
    }
    seen.into_iter()
        .map(|index| graph.unit(index).map(Unit::identity))
        .collect()
}

pub struct CompatiblePlan {
    pub engine_units: usize,
}

pub fn verify(
    engine: &BTreeSet<UnitRecord>,
    consumer: &UnitGraph,
    engine_root: &str,
) -> Result<CompatiblePlan, PlanError> {
    let consumer_records = external_records(consumer)?;
    let changed = engine.difference(&consumer_records).count();
    if changed > 0 {
        return Err(PlanError::EngineUnitsChanged { count: changed });
    }
    let available: BTreeSet<&UnitIdentity> = engine.iter().map(|record| &record.unit).collect();
    let uncached = engine_units(consumer, engine_root)?
        .iter()
        .filter(|unit| !available.contains(unit))
        .count();
    if uncached > 0 {
        return Err(PlanError::EngineUnitsUncached { count: uncached });
    }
    Ok(CompatiblePlan {
        engine_units: engine.len(),
    })
}

#[derive(Debug, thiserror::Error)]
pub enum PlanError {
    #[error(transparent)]
    UnitGraph(#[from] UnitIndexError),
    #[error("{count} engine units changed")]
    EngineUnitsChanged { count: usize },
    #[error("{count} engine units are not cached")]
    EngineUnitsUncached { count: usize },
}

#[cfg(test)]
mod tests;
