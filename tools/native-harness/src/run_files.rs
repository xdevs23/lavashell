// SPDX-License-Identifier: GPL-3.0-only
use crate::error::HarnessError;
use crate::run_label::RunLabel;
use std::fs::File;
use std::path::PathBuf;

pub struct RunFiles {
    directory: PathBuf,
    label: RunLabel,
}

impl RunFiles {
    pub fn in_directory(directory: PathBuf, label: RunLabel) -> Result<Self, HarnessError> {
        std::fs::create_dir_all(&directory).map_err(|source| {
            HarnessError::CreateOutputDirectory {
                path: directory.clone(),
                source,
            }
        })?;
        Ok(Self { directory, label })
    }

    pub fn path(&self, name: &str) -> PathBuf {
        self.directory.join(format!("{}-{name}", self.label))
    }

    pub fn create(&self, name: &str) -> Result<File, HarnessError> {
        let path = self.path(name);
        File::create(&path).map_err(|source| HarnessError::CreateRunFile { path, source })
    }

    pub fn report_path(&self) -> PathBuf {
        self.directory.join(format!("{}.json", self.label))
    }
}
