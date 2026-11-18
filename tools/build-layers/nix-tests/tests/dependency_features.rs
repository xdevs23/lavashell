// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]

use lavashell_nix_tests::evaluation::evaluate;
use lavashell_nix_tests::fixture_error::FixtureError;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use toml::{Table, toml};

#[derive(Serialize)]
struct FeatureArguments<'arguments> {
    root: PathBuf,
    #[serde(rename = "crate")]
    library: &'arguments str,
    dependents: [&'arguments str; 2],
}

fn write_manifest(root: &Path, member: &str, manifest: &Table) -> Result<(), FixtureError> {
    let directory = root.join(member);
    fs::create_dir_all(&directory)?;
    fs::write(directory.join("Cargo.toml"), manifest.to_string())?;
    Ok(())
}

#[test]
fn dependents_contribute_the_features_they_enable_on_normal_dependencies_of_the_crate()
-> Result<(), FixtureError> {
    let directory = tempfile::tempdir()?;
    let root = directory.path().to_owned();
    write_manifest(
        &root,
        "library",
        &toml! {
            [package]
            name = "library"

            [dependencies]
            support = { path = "../support" }

            [dev-dependencies]
            fixtures = { path = "../fixtures" }
        },
    )?;
    write_manifest(
        &root,
        "first",
        &toml! {
            [package]
            name = "first"

            [dependencies]
            support = { path = "../support" }

            [dev-dependencies]
            library = { path = "../library" }
            support = { path = "../support", features = ["testing"] }
            fixtures = { path = "../fixtures", features = ["images"] }
        },
    )?;
    write_manifest(
        &root,
        "second",
        &toml! {
            [package]
            name = "second"

            [dependencies]
            support = { path = "../support", features = ["testing", "logging"] }
            renderer = { path = "../renderer", features = ["vulkan"] }

            [dev-dependencies]
            library = { path = "../library" }
        },
    )?;
    let mut features: Vec<String> = evaluate(
        "dependency-features.nix",
        &FeatureArguments {
            root,
            library: "library",
            dependents: ["first", "second"],
        },
    )?;
    features.sort();
    assert_eq!(features, ["support/logging", "support/testing"]);
    Ok(())
}
