// SPDX-License-Identifier: GPL-3.0-only
use crate::fixture_error::FixtureError;
use std::fs;
use std::path::{Path, PathBuf};

pub fn walk(directory: &Path) -> Result<Vec<PathBuf>, FixtureError> {
    let mut paths = Vec::new();
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        if path.is_dir() {
            paths.extend(walk(&path)?);
        }
        paths.push(path);
    }
    Ok(paths)
}
