// SPDX-License-Identifier: GPL-3.0-only
use lavashell_command_output::json::CommandError;
use lavashell_command_output::nix::NixError;
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum FixtureError {
    #[error("the environment variable PATH is not set")]
    PathUnset,
    #[error(transparent)]
    Nix(#[from] NixError),
    #[error(transparent)]
    File(#[from] std::io::Error),
    #[error(transparent)]
    Command(#[from] CommandError),
    #[error("{path} cannot be copied: {source}")]
    Copy {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{path} is not the expected JSON document: {source}")]
    ReadJson {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("{path} is not the expected TOML document: {source}")]
    ReadToml {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
    #[error("{path} cannot be written as TOML: {source}")]
    WriteToml {
        path: PathBuf,
        #[source]
        source: toml::ser::Error,
    },
}
