// SPDX-License-Identifier: GPL-3.0-only
use std::path::PathBuf;
use std::process::ExitStatus;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("cargo metadata cannot be started")]
    StartMetadata(#[source] std::io::Error),
    #[error("cargo metadata exited with {0}")]
    Metadata(ExitStatus),
    #[error("the output of cargo metadata cannot be read")]
    ReadMetadata(#[source] serde_json::Error),
    #[error("{path} cannot be read")]
    ReadSource {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{path} is not valid Rust")]
    ParseSource {
        path: PathBuf,
        #[source]
        source: syn::Error,
    },
    #[error("{declared_in} declares the module {module}, whose file does not exist")]
    ModuleFile {
        declared_in: PathBuf,
        module: String,
    },
}
