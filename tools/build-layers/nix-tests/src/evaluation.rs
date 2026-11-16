// SPDX-License-Identifier: GPL-3.0-only
use crate::fixture_error::FixtureError;
use crate::repository::repository_root;
use lavashell_command_output::nix::NixEvaluator;
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::path::{Path, PathBuf};

const EXPRESSIONS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/expressions");

#[derive(Serialize)]
struct ExpressionInput<'input, Arguments> {
    nix: PathBuf,
    #[serde(flatten)]
    arguments: &'input Arguments,
}

pub fn evaluate<Arguments: Serialize, Value: DeserializeOwned>(
    expression: &str,
    arguments: &Arguments,
) -> Result<Value, FixtureError> {
    let input = ExpressionInput {
        nix: repository_root()?.join("nix"),
        arguments,
    };
    Ok(NixEvaluator::from_environment()?
        .evaluate(&Path::new(EXPRESSIONS).join(expression), &input)?)
}
