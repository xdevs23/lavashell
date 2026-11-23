// SPDX-License-Identifier: GPL-3.0-only
use crate::fixture_error::FixtureError;
use std::fs;
use std::path::{Path, PathBuf};

pub fn write_files(root: &Path, files: &[(&str, &str)]) -> Result<(), FixtureError> {
    for (relative, contents) in files {
        let path = root.join(relative);
        fs::create_dir_all(path.parent().unwrap_or(root))?;
        fs::write(path, contents)?;
    }
    Ok(())
}

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
