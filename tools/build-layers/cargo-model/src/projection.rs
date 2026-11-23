// SPDX-License-Identifier: GPL-3.0-only
use crate::lock::{CargoLock, LockedPackage};
use crate::manifest::FeatureActivations;
use lavashell_enum_values::enum_with_values;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

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
pub struct StubTarget {
    pub path: String,
    pub main: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SeedHash(String);

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SeedHashes(BTreeMap<String, BTreeMap<Layer, SeedHash>>);

impl SeedHashes {
    pub fn record(&mut self, target: &str, layer: Layer, hash: SeedHash) {
        let _hash_of_previous_refresh: Option<SeedHash> = self
            .0
            .entry(target.to_owned())
            .or_default()
            .insert(layer, hash);
    }

    #[must_use]
    pub fn kept_for_other_targets(&self, obsolete: Layer, target: &str) -> Self {
        Self(
            self.0
                .iter()
                .filter(|(platform, _)| platform.as_str() != target)
                .map(|(platform, layers)| {
                    let kept: BTreeMap<Layer, SeedHash> = layers
                        .iter()
                        .filter(|(layer, _)| **layer != obsolete)
                        .map(|(layer, hash)| (*layer, hash.clone()))
                        .collect();
                    (platform.clone(), kept)
                })
                .filter(|(_, layers)| !layers.is_empty())
                .collect(),
        )
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectionFile {
    pub normal_lock: CargoLock,
    #[serde(default)]
    pub seed_hashes: SeedHashes,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine_lock: Option<CargoLock>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub features: Option<FeatureActivations>,
}

impl ProjectionFile {
    #[must_use]
    pub fn new(normal_lock: CargoLock) -> Self {
        Self {
            normal_lock,
            seed_hashes: SeedHashes::default(),
            engine_lock: None,
            features: None,
        }
    }

    #[must_use]
    pub fn prepared(&self, layer: Layer, target: &str) -> Self {
        let seed_hashes = self
            .seed_hashes
            .kept_for_other_targets(layer.other(), target);
        match layer {
            Layer::Engine => Self {
                seed_hashes,
                ..self.clone()
            },
            Layer::Registry => Self {
                seed_hashes,
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
    #[serde(default, skip_serializing_if = "SeedHashes::is_empty")]
    pub seed_hashes: SeedHashes,
}

impl NativeProjection {
    #[must_use]
    pub fn new(lock: CargoLock) -> Self {
        Self {
            lock,
            seed_hashes: SeedHashes::default(),
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
            seed_hashes: self
                .seed_hashes
                .kept_for_other_targets(Layer::Engine, target),
        }
    }
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
