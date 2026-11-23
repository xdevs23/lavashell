// SPDX-License-Identifier: GPL-3.0-only
use crate::error::Error;
use lavashell_cargo_model::dependency_kind::DependencyKind;
use lavashell_cargo_model::fixed_flag::FixedFlag;
use lavashell_cargo_model::manifest::{ExactVersion, FeatureActivation, FeatureActivations};
use lavashell_cargo_model::unit_graph::{TargetKind, Unit, UnitGraph, UnitIdentity};
use std::collections::HashMap;

fn identity_without_features(unit: &Unit) -> UnitIdentity {
    UnitIdentity {
        features: Vec::new(),
        ..unit.identity()
    }
}

pub fn missing_activations(
    engine: &UnitGraph,
    consumer: &UnitGraph,
) -> Result<FeatureActivations, Error> {
    let mut candidates: HashMap<UnitIdentity, Vec<&Unit>> = HashMap::new();
    for unit in &consumer.units {
        candidates
            .entry(identity_without_features(unit))
            .or_default()
            .push(unit);
    }
    let mut activations = FeatureActivations::new();
    let shared_libraries = engine
        .units
        .iter()
        .filter(|unit| unit.pkg_id.is_external() && unit.target.kind == [TargetKind::Lib]);
    for unit in shared_libraries {
        let matches = candidates
            .get(&identity_without_features(unit))
            .map(Vec::as_slice)
            .unwrap_or_default();
        if matches
            .iter()
            .any(|candidate| candidate.features == unit.features)
        {
            continue;
        }
        let [consumer_unit] = matches else {
            return Err(Error::AmbiguousFeatureContext {
                package: Box::new(unit.pkg_id.clone()),
            });
        };
        let (Some(name), Some(version)) = (unit.pkg_id.name(), unit.pkg_id.version()) else {
            return Err(Error::UnnamedPackage {
                package: Box::new(unit.pkg_id.clone()),
            });
        };
        let kind = match unit.platform {
            None => DependencyKind::Build,
            Some(_) => DependencyKind::Normal,
        };
        let _same_activation_of_other_unit: Option<FeatureActivation> =
            activations.entry(kind).or_default().insert(
                name.to_owned(),
                FeatureActivation {
                    version: ExactVersion(version.to_owned()),
                    default_features: FixedFlag,
                    features: consumer_unit.features.clone(),
                },
            );
    }
    Ok(activations)
}
