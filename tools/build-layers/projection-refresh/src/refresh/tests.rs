// SPDX-License-Identifier: GPL-3.0-only
use super::{Toolkit, refresh_projections};
use crate::cargo::Cargo;
use crate::error::Error;
use crate::files::{read_json, read_text, write_json, write_toml};
use crate::seed::Seeds;
use lavashell_cargo_model::contract::ProjectionContract;
use lavashell_cargo_model::lock::{CargoLock, LockedPackage};
use lavashell_cargo_model::projection::NativeProjection;
use lavashell_command_output::nix::NixEvaluator;
use std::path::Path;
use tempfile::TempDir;

const CONTRACT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../nix/projection-contract.json"
);

#[derive(Debug, thiserror::Error)]
enum TestError {
    #[error(transparent)]
    Refresh(#[from] Error),
    #[error(transparent)]
    Directory(#[from] std::io::Error),
}

#[test]
fn a_refresh_whose_seed_plan_fails_keeps_the_native_projection_it_restarted()
-> Result<(), TestError> {
    let checkout = TempDir::new()?;
    let contract: ProjectionContract = read_json(Path::new(CONTRACT))?;
    let native_path = checkout
        .path()
        .join(&contract.projection_directory)
        .join("native.json");
    write_toml(
        &checkout.path().join("Cargo.lock"),
        &CargoLock {
            version: 4,
            package: Vec::new(),
        },
    )?;
    write_json(
        &native_path,
        &NativeProjection::new(CargoLock {
            version: 4,
            package: vec![LockedPackage {
                name: "toml".to_owned(),
                version: "1.0.0".to_owned(),
                source: Some("registry+https://github.com/rust-lang/crates.io-index".to_owned()),
                checksum: None,
                dependencies: Vec::new(),
            }],
        }),
    )?;
    let stored = read_text(&native_path)?;
    let nix = NixEvaluator::from_environment().map_err(Error::Nix)?;
    let cargo = Cargo {
        target: contract.target.clone(),
        profile: contract.profile.clone(),
    };
    let toolkit = Toolkit {
        cargo: &cargo,
        seeds: Seeds {
            nix: &nix,
            root: checkout.path(),
            contract: &contract,
        },
        contract: &contract,
        root: checkout.path(),
    };
    assert!(matches!(
        refresh_projections(&toolkit, &[]),
        Err(Error::Nix(_))
    ));
    assert_eq!(read_text(&native_path)?, stored);
    Ok(())
}
