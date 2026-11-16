// SPDX-License-Identifier: GPL-3.0-only
use crate::json::{CommandError, read_json_output};
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::path::{Path, PathBuf};
use std::process::Command;

const NIXPKGS_VARIABLE: &str = "LAVASHELL_NIXPKGS";

#[derive(Debug, thiserror::Error)]
pub enum NixError {
    #[error("the environment variable LAVASHELL_NIXPKGS is not set")]
    NixpkgsUnset,
    #[error("the arguments of the Nix expression {path} cannot be encoded: {source}")]
    EncodeArguments {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error(transparent)]
    Command(#[from] CommandError),
}

pub struct NixEvaluator {
    nixpkgs: PathBuf,
}

#[derive(Serialize)]
struct EvaluationInput<'input, Arguments> {
    nixpkgs: &'input Path,
    #[serde(flatten)]
    arguments: &'input Arguments,
}

impl NixEvaluator {
    pub fn from_environment() -> Result<Self, NixError> {
        let nixpkgs = std::env::var_os(NIXPKGS_VARIABLE).ok_or(NixError::NixpkgsUnset)?;
        Ok(Self {
            nixpkgs: PathBuf::from(nixpkgs),
        })
    }

    #[must_use]
    pub fn nixpkgs(&self) -> &Path {
        &self.nixpkgs
    }

    pub fn evaluate<Arguments: Serialize, Value: DeserializeOwned>(
        &self,
        file: &Path,
        arguments: &Arguments,
    ) -> Result<Value, NixError> {
        let input = EvaluationInput {
            nixpkgs: &self.nixpkgs,
            arguments,
        };
        let encoded =
            serde_json::to_string(&input).map_err(|source| NixError::EncodeArguments {
                path: file.to_owned(),
                source,
            })?;
        Ok(read_json_output(
            Command::new("nix-instantiate")
                .args(["--eval", "--strict", "--json", "--read-write-mode"])
                .arg(file)
                .args(["--argstr", "arguments"])
                .arg(encoded),
        )?)
    }
}
