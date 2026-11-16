// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]

use std::error::Error;
use std::path::PathBuf;
use std::process::ExitCode;
use std::{env, fs, io};

use lavashell_core::error::message_chain;

#[derive(Debug, thiserror::Error)]
pub enum BuildScriptError {
    #[error("{} is not set. Build inside `nix develop`, or set it to {}.", .0.name, .0.package)]
    PackageNotSet(PackageVariable),
    #[error("cargo does not set OUT_DIR. Run the build script through cargo.")]
    OutputDirectoryNotSet,
    #[error("cannot write {}", .file.display())]
    Write { file: PathBuf, source: io::Error },
}

#[derive(Clone, Copy, Debug)]
pub struct PackageVariable {
    pub name: &'static str,
    pub package: &'static str,
}

impl PackageVariable {
    pub fn path(self) -> Result<PathBuf, BuildScriptError> {
        env::var_os(self.name)
            .map(PathBuf::from)
            .ok_or(BuildScriptError::PackageNotSet(self))
    }

    pub fn rerun_if_variable_changed(self) {
        build_rs::output::rerun_if_env_changed(self.name);
    }
}

pub fn output_directory() -> Result<PathBuf, BuildScriptError> {
    env::var_os("OUT_DIR")
        .map(PathBuf::from)
        .ok_or(BuildScriptError::OutputDirectoryNotSet)
}

pub fn write(file: PathBuf, contents: impl AsRef<[u8]>) -> Result<(), BuildScriptError> {
    fs::write(&file, contents).map_err(|source| BuildScriptError::Write { file, source })
}

#[must_use]
pub fn run<E: Error>(generate: impl FnOnce() -> Result<(), E>) -> ExitCode {
    match generate() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{}", message_chain(error));
            ExitCode::FAILURE
        }
    }
}
