// SPDX-License-Identifier: GPL-3.0-only
use crate::error::Error;
use crate::files::{read_text, write_text};
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

pub struct ProjectionStage {
    checkout: PathBuf,
    staged: TempDir,
}

impl ProjectionStage {
    pub fn copy_of(checkout: PathBuf) -> Result<Self, Error> {
        let staged = tempfile::tempdir().map_err(Error::TemporaryDirectory)?;
        copy_files(&checkout, staged.path())?;
        Ok(Self { checkout, staged })
    }

    pub fn path(&self) -> &Path {
        self.staged.path()
    }

    pub fn publish(self) -> Result<(), Error> {
        copy_files(self.staged.path(), &self.checkout)
    }
}

fn copy_files(from: &Path, to: &Path) -> Result<(), Error> {
    let listing_failed = |source: std::io::Error| Error::ListDirectory {
        path: from.to_owned(),
        source,
    };
    fs::read_dir(from)
        .map_err(listing_failed)?
        .try_for_each(|entry| {
            let entry = entry.map_err(listing_failed)?;
            write_text(&to.join(entry.file_name()), &read_text(&entry.path())?)
        })
}
