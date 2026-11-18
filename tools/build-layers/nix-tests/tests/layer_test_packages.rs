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
}

impl CheckPhase {
    fn tested_packages(&self) -> Vec<&str> {
        if !self.do_check {
            return Vec::new();
        }
        self.cargo_test_flags
            .windows(2)
            .filter_map(|pair| match pair {
                [flag, package] if flag == PACKAGE_FLAG => Some(package.as_str()),
                _ => None,
            })
            .collect()
    }
}

#[derive(Deserialize)]
struct Example {
    closure: Vec<String>,
    derivations: Vec<CheckPhase>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CheckPhases {
    native_names: Vec<String>,
    members: Vec<String>,
    checks: Vec<CheckPhase>,
    dependency_layers: Vec<CheckPhase>,
    support: CheckPhase,
    examples: BTreeMap<String, Example>,
}

fn check_phases() -> Result<CheckPhases, FixtureError> {
    let directory = tempfile::tempdir()?;
    let root = directory.path().join("repository");
    copy_repository(&root)?;
    evaluate(EXPRESSION, &TreeArguments { root })
}

#[test]
fn the_registry_and_engine_layers_run_no_tests() -> Result<(), FixtureError> {
    for layer in check_phases()?.dependency_layers {
        assert!(!layer.do_check, "{:?}", layer.cargo_test_flags);
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
