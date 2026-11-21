// SPDX-License-Identifier: GPL-3.0-only
use crate::error::Error;
use crate::files::{create_directory, read_bytes, write_bytes};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::path::Path;

#[cfg(test)]
mod tests;

type ProjectionFiles = BTreeMap<OsString, Vec<u8>>;

pub fn publish_staged_refresh<Output>(
    projections: &Path,
    refresh: impl FnOnce(&Path) -> Result<Output, Error>,
) -> Result<Output, Error> {
    create_directory(projections)?;
    let stored = read_projection_files(projections)?;
    let stage = tempfile::Builder::new()
        .prefix(".refresh-")
        .tempdir_in(projections)
        .map_err(Error::TemporaryDirectory)?;
    stored
        .iter()
        .try_for_each(|(name, contents)| write_bytes(&stage.path().join(name), contents))?;
    let output = refresh(stage.path())?;
    read_projection_files(stage.path())?
        .into_iter()
        .filter(|(name, contents)| stored.get(name) != Some(contents))
        .try_for_each(|(name, _)| {
            let published = projections.join(&name);
            fs::rename(stage.path().join(&name), &published).map_err(|source| {
                Error::PublishProjection {
                    path: published,
                    source,
                }
            })
        })?;
    Ok(output)
}

fn read_projection_files(directory: &Path) -> Result<ProjectionFiles, Error> {
    let listing_failed = |source| Error::ListDirectory {
        path: directory.to_owned(),
        source,
    };
    fs::read_dir(directory)
        .map_err(listing_failed)?
        .map(|entry| entry.map_err(listing_failed))
        .filter(|entry| entry.as_ref().map_or(true, |entry| entry.path().is_file()))
        .map(|entry| {
            let entry = entry?;
            Ok((entry.file_name(), read_bytes(&entry.path())?))
        })
        .collect()
}
