// SPDX-License-Identifier: GPL-3.0-only
use crate::CompositorRenderer;
use crate::error::HarnessError;
use serde::Serialize;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

#[derive(Serialize)]
pub struct Report {
    pub flake: String,
    pub all_wayland_apps: bool,
    pub requested_renderer: Option<CompositorRenderer>,
    pub examples: Vec<AppResult>,
}

#[derive(Serialize)]
pub struct AppResult {
    pub name: String,
    pub seconds: f64,
    pub screenshot: Option<PathBuf>,
    pub executable: PathBuf,
}

impl Report {
    pub fn write(&self, path: &Path) -> Result<(), HarnessError> {
        let write_error = |source| HarnessError::WriteReport {
            path: path.to_owned(),
            source,
        };
        let mut file = BufWriter::new(File::create(path).map_err(write_error)?);
        serde_json::to_writer_pretty(&mut file, self).map_err(HarnessError::EncodeReport)?;
        writeln!(file)
            .and_then(|()| file.flush())
            .map_err(write_error)
    }
}

#[cfg(test)]
mod tests;
