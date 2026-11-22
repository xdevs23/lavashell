// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]

use lavashell_nix_tests::evaluation::evaluate;
use lavashell_nix_tests::fixture_error::FixtureError;
use lavashell_nix_tests::repository::copy_repository;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

const EXPRESSION: &str = "layer-test-packages.nix";
const PACKAGE_FLAG: &str = "--package";
const WORKSPACE_FLAG: &str = "--workspace";
const COMPILE_ONLY_FLAG: &str = "--no-run";

#[derive(Serialize)]
struct TreeArguments {
    root: PathBuf,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CheckPhase {
    drv_path: String,
    do_check: bool,
    cargo_test_flags: Vec<String>,
    cargo_clippy_flags: Vec<String>,
}

fn packages_selected_by(flags: &[String]) -> Vec<&str> {
    flags
        .windows(2)
        .filter_map(|pair| match pair {
            [flag, package] if flag == PACKAGE_FLAG => Some(package.as_str()),
            _ => None,
        })
        .collect()
}

impl CheckPhase {
    fn compiled_test_packages(&self) -> Vec<&str> {
        if !self.do_check {
            return Vec::new();
        }
        packages_selected_by(&self.cargo_test_flags)
    }

    fn tested_packages(&self) -> Vec<&str> {
        if self
            .cargo_test_flags
            .iter()
            .any(|flag| flag == COMPILE_ONLY_FLAG)
        {
            return Vec::new();
        }
        self.compiled_test_packages()
    }

    fn lints(&self, package: &str) -> bool {
        self.cargo_clippy_flags
            .iter()
            .any(|flag| flag == WORKSPACE_FLAG)
            || packages_selected_by(&self.cargo_clippy_flags).contains(&package)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Example {
    package: String,
    closure: Vec<String>,
    derivations: Vec<CheckPhase>,
    library_layers: Vec<String>,
    has_engine: bool,
    unified: Option<UnifiedLayer>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UnifiedLayer {
    layer: CheckPhase,
    layers_above: Vec<CheckPhase>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LibraryLayer {
    package: String,
    layer: String,
    has_engine: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CheckPhases {
    native_names: Vec<String>,
    members: Vec<String>,
    checks: Vec<CheckPhase>,
    dependency_layers: Vec<CheckPhase>,
    registry: CheckPhase,
    support: CheckPhase,
    library_layers: Vec<LibraryLayer>,
    examples: BTreeMap<String, Example>,
}

fn check_phases() -> Result<CheckPhases, FixtureError> {
    let directory = tempfile::tempdir()?;
    let root = directory.path().join("repository");
    copy_repository(&root)?;
    evaluate(EXPRESSION, &TreeArguments { root })
}

#[test]
fn the_dependency_layers_run_no_tests() -> Result<(), FixtureError> {
    for layer in check_phases()?.dependency_layers {
        assert!(
            layer.tested_packages().is_empty(),
            "{:?}",
            layer.cargo_test_flags
        );
    }
    Ok(())
}

fn assert_compiles_and_lints_the_tests_above(stub: &CheckPhase, above: &[CheckPhase]) {
    let compiled: BTreeSet<&str> = stub.compiled_test_packages().into_iter().collect();
    let tested: BTreeSet<&str> = above.iter().flat_map(CheckPhase::tested_packages).collect();
    assert_eq!(compiled, tested, "{}", stub.drv_path);
    assert!(
        tested.iter().all(|package| stub.lints(package)),
        "{}",
        stub.drv_path
    );
}

#[test]
fn the_registry_layer_compiles_and_lints_the_tests_of_the_support_layer() -> Result<(), FixtureError>
{
    let phases = check_phases()?;
    assert_compiles_and_lints_the_tests_above(&phases.registry, &[phases.support]);
    Ok(())
}

#[test]
fn every_unified_layer_compiles_and_lints_the_tests_of_the_layers_above_it()
-> Result<(), FixtureError> {
    let unified_layers: Vec<UnifiedLayer> = check_phases()?
        .examples
        .into_values()
        .filter_map(|example| example.unified)
        .collect();
    assert!(!unified_layers.is_empty());
    for unified in &unified_layers {
        assert_compiles_and_lints_the_tests_above(&unified.layer, &unified.layers_above);
    }
    Ok(())
}

#[test]
fn the_support_layer_tests_every_crate_of_its_projection() -> Result<(), FixtureError> {
    let phases = check_phases()?;
    let tested: BTreeSet<&str> = phases.support.tested_packages().into_iter().collect();
    let native: BTreeSet<&str> = phases.native_names.iter().map(String::as_str).collect();
    assert_eq!(tested, native);
    Ok(())
}

#[test]
fn every_local_crate_of_an_example_is_tested_once_in_the_whole_build() -> Result<(), FixtureError> {
    let phases = check_phases()?;
    let build: BTreeMap<&str, &CheckPhase> = phases
        .examples
        .values()
        .flat_map(|example| &example.derivations)
        .map(|derivation| (derivation.drv_path.as_str(), derivation))
        .collect();
    let tested: Vec<&str> = build
        .values()
        .flat_map(|derivation| derivation.tested_packages())
        .collect();
    for (name, example) in &phases.examples {
        let runs: BTreeMap<&str, usize> = example
            .closure
            .iter()
            .map(|member| {
                let count = tested.iter().filter(|package| **package == member).count();
                (member.as_str(), count)
            })
            .collect();
        assert!(runs.values().all(|count| *count == 1), "{name}: {runs:?}");
    }
    Ok(())
}

#[test]
fn every_package_linking_a_library_crate_copies_its_layer_and_has_an_engine_layer_like_it()
-> Result<(), FixtureError> {
    let phases = check_phases()?;
    for library in &phases.library_layers {
        let linking = phases.examples.iter().filter(|(_, example)| {
            example.package != library.package && example.closure.contains(&library.package)
        });
        for (name, example) in linking {
            assert!(
                example.library_layers.contains(&library.layer),
                "{name} does not copy the layer of {}",
                library.package
            );
            assert!(
                example.has_engine || !library.has_engine,
                "{name} has no engine layer, although {} has one",
                library.package
            );
        }
    }
    Ok(())
}

#[test]
fn every_flake_check_lints_each_crate_it_tests() -> Result<(), FixtureError> {
    for check in check_phases()?.checks {
        let unlinted: Vec<&str> = check
            .tested_packages()
            .into_iter()
            .filter(|package| !check.lints(package))
            .collect();
        assert_eq!(unlinted, Vec::<&str>::new(), "{}", check.drv_path);
    }
    Ok(())
}

#[test]
fn every_workspace_member_is_tested_by_a_flake_check() -> Result<(), FixtureError> {
    let phases = check_phases()?;
    let tested: BTreeSet<&str> = phases
        .checks
        .iter()
        .flat_map(CheckPhase::tested_packages)
        .collect();
    let untested: Vec<&String> = phases
        .members
        .iter()
        .filter(|member| !tested.contains(member.as_str()))
        .collect();
    assert!(untested.is_empty(), "{untested:?}");
    Ok(())
}
