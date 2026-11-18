// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]

use lavashell_cargo_model::lock::{CargoLock, LockedPackage};
use lavashell_nix_tests::evaluation::evaluate;
use lavashell_nix_tests::fixture_error::FixtureError;
use serde::Serialize;
use toml::{Table, toml};

#[derive(Serialize)]
struct PatchArguments {
    patch: Table,
    lock: CargoLock,
}

fn git() -> Table {
    toml! {
        git = "https://example.invalid/repo"
        rev = "0123abc"
    }
}

fn kept(patch: Table, locked: &[&str]) -> Result<Table, FixtureError> {
    let package = locked
        .iter()
        .map(|name| LockedPackage {
            name: (*name).to_owned(),
            version: "1.0.0".to_owned(),
            source: None,
            checksum: None,
            dependencies: Vec::new(),
        })
        .collect();
    let lock = CargoLock {
        version: 4,
        package,
    };
    evaluate("cargo-patch.nix", &PatchArguments { patch, lock })
}

fn registry(name: &str, patch: Table) -> Table {
    Table::from_iter([(name.to_owned(), patch.into())])
}

#[test]
fn the_patch_of_a_locked_crate_is_kept() -> Result<(), FixtureError> {
    let patch = registry("crates-io", registry("locked", git()));
    assert_eq!(kept(patch.clone(), &["locked"])?, patch);
    Ok(())
}

#[test]
fn the_patch_of_a_crate_absent_from_the_lock_is_dropped_with_its_empty_table()
-> Result<(), FixtureError> {
    let mut patch = registry("crates-io", registry("locked", git()));
    patch.extend(registry("other-registry", registry("absent", git())));
    assert_eq!(
        kept(patch, &["locked"])?,
        registry("crates-io", registry("locked", git()))
    );
    Ok(())
}

#[test]
fn a_renamed_patch_is_matched_by_its_package_name() -> Result<(), FixtureError> {
    let mut renamed = git();
    let _replaced: Option<toml::Value> = renamed.insert("package".to_owned(), "locked".into());
    let aliased = registry("crates-io", registry("alias", renamed.clone()));
    assert_eq!(kept(aliased.clone(), &["locked"])?, aliased);
    let misnamed = registry("crates-io", registry("locked-alias", renamed));
    assert_eq!(kept(misnamed, &["locked-alias"])?, Table::new());
    Ok(())
}
