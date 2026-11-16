// SPDX-License-Identifier: GPL-3.0-only
use crate::fixture_error::FixtureError;
use lavashell_cargo_model::contract::ProjectionContract;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

const CONTRACT: &str = "nix/projection-contract.json";
const COPIED_FILES: [&str; 4] = ["Cargo.toml", "Cargo.lock", "COPYING", "about.toml"];
const COPIED_DIRECTORIES: [&str; 3] = ["crates", "examples", "tools"];
const SKIPPED_NAMES: [&str; 2] = ["node_modules", "target"];
const WRITABLE_DIRECTORY: u32 = 0o755;
const WRITABLE_FILE: u32 = 0o644;

pub fn repository_root() -> Result<PathBuf, FixtureError> {
    Ok(fs::canonicalize(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../.."
    ))?)
}

pub fn projection_contract() -> Result<ProjectionContract, FixtureError> {
    let path = repository_root()?.join(CONTRACT);
    serde_json::from_str(&fs::read_to_string(&path)?)
        .map_err(|source| FixtureError::ReadJson { path, source })
}

fn copy_file(source: &Path, destination: &Path) -> Result<(), FixtureError> {
    let _bytes: u64 = fs::copy(source, destination).map_err(|source_error| FixtureError::Copy {
        path: source.to_owned(),
        source: source_error,
    })?;
    Ok(fs::set_permissions(
        destination,
        fs::Permissions::from_mode(WRITABLE_FILE),
    )?)
}

fn copy_tree(source: &Path, destination: &Path) -> Result<(), FixtureError> {
    fs::create_dir(destination)?;
    fs::set_permissions(destination, fs::Permissions::from_mode(WRITABLE_DIRECTORY))?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        if SKIPPED_NAMES.iter().any(|name| entry.file_name() == *name) {
            continue;
        }
        let (from, to) = (entry.path(), destination.join(entry.file_name()));
        if fs::metadata(&from)?.is_dir() {
            copy_tree(&from, &to)?;
        } else {
            copy_file(&from, &to)?;
        }
    }
    Ok(())
}

pub fn copy_repository(destination: &Path) -> Result<(), FixtureError> {
    let root = repository_root()?;
    fs::create_dir(destination)?;
    for name in COPIED_FILES {
        copy_file(&root.join(name), &destination.join(name))?;
    }
    let projections = projection_contract()?.projection_directory;
    fs::create_dir_all(
        destination
            .join(&projections)
            .parent()
            .unwrap_or(destination),
    )?;
    for name in COPIED_DIRECTORIES
        .iter()
        .copied()
        .chain([projections.as_str()])
    {
        copy_tree(&root.join(name), &destination.join(name))?;
    }
    Ok(())
}
