// SPDX-License-Identifier: GPL-3.0-only
use crate::error::NextStep;
use lavashell_plan_check::stub_layer_reuse::{RecompileCause, RecompiledUnit};
use std::fmt;

pub struct RecompileReport<'report, 'records>(pub &'report RecompiledUnit<'records>);

impl fmt::Display for RecompileReport<'_, '_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let RecompiledUnit {
            unit,
            stub_layer_record,
            cause,
        } = self.0;
        let target = &unit.target.name;
        let package = &unit.package_id;
        let features = unit.features.join(", ");
        let record = stub_layer_record.display();
        match cause {
            RecompileCause::UnplannedVariant => write!(
                formatter,
                "This build compiled the target {target} of {package} with the features [{features}], \
                 which the stub layer recorded in {record} built only with other features."
            ),
            RecompileCause::DependenciesResolvedDifferently => write!(
                formatter,
                "This build compiled the target {target} of {package} with the features [{features}] \
                 under another hash than the stub layer recorded in {record}, because its dependencies resolved differently."
            ),
            RecompileCause::PlannedUnitRebuilt => write!(
                formatter,
                "This build compiled the target {target} of {package} with the features [{features}] again, \
                 although the stub layer recorded in {record} built that same unit under the same hash."
            ),
        }?;
        write!(formatter, " {}", NextStep::from(*cause))
    }
}
