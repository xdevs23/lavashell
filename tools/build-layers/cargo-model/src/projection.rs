// SPDX-License-Identifier: GPL-3.0-only
use crate::lock::{CargoLock, LockedPackage};
use crate::manifest::{FeatureActivations, StubManifest};
use crate::unit_graph::UnitRecord;
use lavashell_enum_values::enum_with_values;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

enum_with_values! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
    #[serde(rename_all = "lowercase")]
    pub enum Layer {
        Engine,
        Registry,
    }

    pub const ALL;
}

impl Layer {
    #[must_use]
    pub fn other(self) -> Self {
        match self {
            Self::Engine => Self::Registry,
            Self::Registry => Self::Engine,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Selection {
    pub target: String,
    pub profile: String,
    pub packages: Vec<String>,
    pub features: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StubTarget {
    pub path: String,
    pub main: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanonicalCrate {
    pub member: String,
    pub manifest: StubManifest,
    pub targets: Vec<StubTarget>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceManifest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<toml::Table>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patch: Option<toml::Table>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replace: Option<toml::Table>,
    pub workspace: WorkspaceSection,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceSection {
    pub resolver: String,
    pub members: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CanonicalPlan {
    pub schema: u32,
    pub selection: Selection,
    pub manifest: WorkspaceManifest,
    pub crates: Vec<CanonicalCrate>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LockedPlan {
    #[serde(flatten)]
    pub plan: CanonicalPlan,
    pub lock: CargoLock,
    pub external_units: BTreeSet<UnitRecord>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LayerPlan {
    pub seed_hash: String,
    pub plan: LockedPlan,
}

pub type DependencyPlans = BTreeMap<String, BTreeMap<Layer, LayerPlan>>;

fn plans_kept_for_other_targets(
    plans: &DependencyPlans,
    obsolete: Layer,
    target: &str,
) -> DependencyPlans {
    plans
        .iter()
        .filter(|(platform, _)| platform.as_str() != target)
        .map(|(platform, layers)| {
            let kept: BTreeMap<Layer, LayerPlan> = layers
                .iter()
                .filter(|(layer, _)| **layer != obsolete)
                .map(|(layer, plan)| (*layer, plan.clone()))
                .collect();
            (platform.clone(), kept)
        })
        .filter(|(_, layers)| !layers.is_empty())
        .collect()
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectionFile {
    pub normal_lock: CargoLock,
    #[serde(default)]
    pub dependency_plans: DependencyPlans,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine_lock: Option<CargoLock>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub features: Option<FeatureActivations>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_units: Option<BTreeSet<UnitRecord>>,
}

impl ProjectionFile {
    #[must_use]
    pub fn new(normal_lock: CargoLock) -> Self {
        Self {
            normal_lock,
            dependency_plans: DependencyPlans::new(),
            engine_lock: None,
            features: None,
            external_units: None,
        }
    }

    #[must_use]
    pub fn prepared(&self, layer: Layer, target: &str) -> Self {
        let dependency_plans =
            plans_kept_for_other_targets(&self.dependency_plans, layer.other(), target);
        match layer {
            Layer::Engine => Self {
                dependency_plans,
                ..self.clone()
            },
            Layer::Registry => Self {
                dependency_plans,
                ..Self::new(self.normal_lock.clone())
            },
        }
    }

    #[must_use]
    pub fn find_crate_unresolved_in(&self, lock: &CargoLock) -> Option<&LockedPackage> {
        std::iter::once(&self.normal_lock)
            .chain(&self.engine_lock)
            .find_map(|stored| stored.find_crate_unresolved_in(lock))
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeProjection {
    pub lock: CargoLock,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub dependency_plans: DependencyPlans,
}

impl NativeProjection {
    #[must_use]
    pub fn new(lock: CargoLock) -> Self {
        Self {
            lock,
            dependency_plans: DependencyPlans::new(),
        }
    }

    #[must_use]
    pub fn find_crate_unresolved_in(&self, lock: &CargoLock) -> Option<&LockedPackage> {
        self.lock.find_crate_unresolved_in(lock)
    }

    #[must_use]
    pub fn prepared(&self, lock: CargoLock, target: &str) -> Self {
        Self {
            lock,
            dependency_plans: plans_kept_for_other_targets(
                &self.dependency_plans,
                Layer::Engine,
                target,
            ),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineProjection {
    pub external_units: BTreeSet<UnitRecord>,
}

#[cfg(test)]
mod tests {
    use super::ProjectionFile;
    use crate::lock::tests::registry_lock;

    #[test]
    fn finds_a_crate_unresolved_in_the_engine_lock() {
        let cargo_lock = registry_lock(&[("serde", "1.0.0")]);
        let projection = ProjectionFile {
            engine_lock: Some(registry_lock(&[("toml", "1.0.0")])),
            ..ProjectionFile::new(cargo_lock.clone())
        };
        assert_eq!(
            projection
                .find_crate_unresolved_in(&cargo_lock)
                .map(|package| package.name.as_str()),
            Some("toml")
        );
    }
}
