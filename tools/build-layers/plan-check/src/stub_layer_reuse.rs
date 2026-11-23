// SPDX-License-Identifier: GPL-3.0-only
use lavashell_cargo_model::build_message::{CompiledUnit, CompilerArtifact};
use lavashell_cargo_model::package_id::PackageId;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

pub struct StubLayerRecord {
    path: PathBuf,
    third_party_units: BTreeMap<PackageId, BTreeSet<CompiledUnit>>,
    third_party_outputs: BTreeSet<PathBuf>,
}

impl StubLayerRecord {
    pub fn new(
        path: PathBuf,
        artifacts: impl IntoIterator<Item = CompilerArtifact>,
    ) -> Result<Self, StubLayerRecordError> {
        let mut third_party_units = BTreeMap::<PackageId, BTreeSet<CompiledUnit>>::new();
        let mut third_party_outputs = BTreeSet::new();
        for artifact in artifacts
            .into_iter()
            .filter(|artifact| artifact.unit.package_id.is_external())
        {
            third_party_outputs.extend(artifact.output_names());
            let _first_record: bool = third_party_units
                .entry(artifact.unit.package_id.clone())
                .or_default()
                .insert(artifact.unit);
        }
        if third_party_units.is_empty() {
            return Err(StubLayerRecordError { path });
        }
        Ok(Self {
            path,
            third_party_units,
            third_party_outputs,
        })
    }

    fn wrote_outputs(&self, artifact: &CompilerArtifact) -> bool {
        artifact
            .output_names()
            .any(|name| self.third_party_outputs.contains(&name))
    }

    fn built_unit(&self, artifact: &CompilerArtifact) -> bool {
        self.third_party_units
            .get(&artifact.unit.package_id)
            .is_some_and(|units| units.contains(&artifact.unit))
    }

    fn built_package(&self, artifact: &CompilerArtifact) -> bool {
        self.third_party_units
            .contains_key(&artifact.unit.package_id)
    }
}

#[derive(Debug, thiserror::Error)]
#[error("the stub layer record {path} names no third-party unit")]
pub struct StubLayerRecordError {
    pub path: PathBuf,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum CheckedBuild {
    StubLayer,
    Consumer,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecompileCause {
    UnplannedVariant,
    DependenciesResolvedDifferently,
    PlannedUnitRebuilt,
}

pub struct RecompiledUnit<'records> {
    pub unit: &'records CompiledUnit,
    pub stub_layer_record: &'records Path,
    pub cause: RecompileCause,
}

pub fn recompiled_units<'records>(
    build: &'records [CompilerArtifact],
    stub_layers: &'records [StubLayerRecord],
    checked_build: CheckedBuild,
) -> impl Iterator<Item = RecompiledUnit<'records>> {
    build
        .iter()
        .filter(|artifact| !artifact.fresh)
        .filter_map(move |artifact| recompile_of(artifact, stub_layers, checked_build))
}

fn recompile_of<'records>(
    artifact: &'records CompilerArtifact,
    stub_layers: &'records [StubLayerRecord],
    checked_build: CheckedBuild,
) -> Option<RecompiledUnit<'records>> {
    let find_stub_layer = |cause, built: fn(&StubLayerRecord, &CompilerArtifact) -> bool| {
        stub_layers
            .iter()
            .find(|layer| built(layer, artifact))
            .map(|layer| (layer, cause))
    };
    let (stub_layer, cause) = find_stub_layer(
        RecompileCause::PlannedUnitRebuilt,
        StubLayerRecord::wrote_outputs,
    )
    .or_else(|| {
        find_stub_layer(
            RecompileCause::DependenciesResolvedDifferently,
            StubLayerRecord::built_unit,
        )
    })
    .or_else(|| match checked_build {
        CheckedBuild::StubLayer => None,
        CheckedBuild::Consumer => find_stub_layer(
            RecompileCause::UnplannedVariant,
            StubLayerRecord::built_package,
        ),
    })?;
    Some(RecompiledUnit {
        unit: &artifact.unit,
        stub_layer_record: &stub_layer.path,
        cause,
    })
}
