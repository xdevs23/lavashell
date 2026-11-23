// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]

use lavashell_nix_tests::evaluation::evaluate;
use lavashell_nix_tests::fixture_error::FixtureError;
use lavashell_nix_tests::repository::copy_repository;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize)]
struct TreeArguments {
    root: PathBuf,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Build {
    drv_path: String,
    reused_stub_layers: Vec<Vec<String>>,
}

#[derive(Deserialize)]
struct ReferenceBuilds {
    registry: Build,
    engine: Build,
    unified: Build,
    framework: Build,
    package: Build,
}

fn reference_builds() -> Result<ReferenceBuilds, FixtureError> {
    let directory = tempfile::tempdir()?;
    let root = directory.path().join("repository");
    copy_repository(&root)?;
    evaluate("stub-layer-reuse-checks.nix", &TreeArguments { root })
}

#[test]
fn the_registry_and_engine_layers_check_no_reuse() -> Result<(), FixtureError> {
    let builds = reference_builds()?;
    for stub_layer in [builds.registry, builds.engine] {
        assert!(
            stub_layer.reused_stub_layers.is_empty(),
            "{}",
            stub_layer.drv_path
        );
    }
    Ok(())
}

#[test]
fn the_unified_layer_checks_the_reuse_of_the_engine_layer_only() -> Result<(), FixtureError> {
    let builds = reference_builds()?;
    assert_eq!(
        builds.unified.reused_stub_layers,
        [[builds.engine.drv_path]]
    );
    Ok(())
}

#[test]
fn the_framework_layer_and_the_package_on_an_engine_check_the_reuse_of_every_stub_layer()
-> Result<(), FixtureError> {
    let builds = reference_builds()?;
    let stub_layers = vec![vec![
        builds.registry.drv_path,
        builds.engine.drv_path,
        builds.unified.drv_path,
    ]];
    assert_eq!(builds.framework.reused_stub_layers, stub_layers);
    assert_eq!(builds.package.reused_stub_layers, stub_layers);
    Ok(())
}
