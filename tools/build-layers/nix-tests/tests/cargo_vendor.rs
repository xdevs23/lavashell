// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]

use lavashell_cargo_model::lock::{CargoLock, LockedPackage};
use lavashell_command_output::json::CommandError;
use lavashell_command_output::nix::NixError;
use lavashell_nix_tests::evaluation::evaluate;
use lavashell_nix_tests::fixture_error::FixtureError;
use lavashell_nix_tests::toml_files::write_toml;
use serde::Serialize;
use std::ffi::OsStr;
use std::path::PathBuf;

const CRATES_IO: &str = "registry+https://github.com/rust-lang/crates.io-index";
const GIT: &str = "git+https://example.invalid/repo?rev=0123abc#0123abc";
const HASH: &str = "sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct VendoredCommit {
    cargo_source: &'static str,
    hash: &'static str,
}

#[derive(Serialize)]
struct VendorArguments {
    lock: PathBuf,
    commits: Vec<VendoredCommit>,
}

fn locked(name: &str, source: &str) -> LockedPackage {
    LockedPackage {
        name: name.to_owned(),
        version: "1.0.0".to_owned(),
        source: Some(source.to_owned().into()),
        checksum: (source == CRATES_IO).then(|| "0".repeat(64)),
        dependencies: Vec::new(),
    }
}

fn vendored_derivation(
    packages: &[(&str, &str)],
    commits: Vec<VendoredCommit>,
) -> Result<PathBuf, FixtureError> {
    let directory = tempfile::tempdir()?;
    let lock = directory.path().join("Cargo.lock");
    let package = packages
        .iter()
        .map(|(name, source)| locked(name, source))
        .collect();
    write_toml(
        &lock,
        &CargoLock {
            version: 4,
            package,
        },
    )?;
    evaluate("cargo-vendor.nix", &VendorArguments { lock, commits })
}

#[test]
fn crates_from_crates_io_and_a_known_commit_are_vendored() -> Result<(), FixtureError> {
    let commits = vec![VendoredCommit {
        cargo_source: GIT,
        hash: HASH,
    }];
    let derivation = vendored_derivation(
        &[("registry-crate", CRATES_IO), ("git-crate", GIT)],
        commits,
    )?;
    assert_eq!(derivation.extension(), Some(OsStr::new("drv")));
    Ok(())
}

#[test]
fn a_lock_without_git_crates_needs_no_commits() -> Result<(), FixtureError> {
    let derivation = vendored_derivation(&[("registry-crate", CRATES_IO)], Vec::new())?;
    assert_eq!(derivation.extension(), Some(OsStr::new("drv")));
    Ok(())
}

#[test]
fn a_commit_without_a_hash_fails_evaluation_naming_the_crate() {
    let failure = vendored_derivation(&[("git-crate", GIT)], Vec::new());
    assert!(matches!(
        failure,
        Err(FixtureError::Nix(NixError::Command(CommandError::Failed { stderr, .. })))
            if stderr.contains("git-crate")
    ));
}
