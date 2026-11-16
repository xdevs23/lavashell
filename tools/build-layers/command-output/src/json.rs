// SPDX-License-Identifier: GPL-3.0-only
use serde::de::DeserializeOwned;
use std::fmt;
use std::process::Command;

#[derive(Debug)]
pub struct CommandLine(Vec<String>);

impl From<&Command> for CommandLine {
    fn from(command: &Command) -> Self {
        Self(
            std::iter::once(command.get_program())
                .chain(command.get_args())
                .map(|word| word.to_string_lossy().into_owned())
                .collect(),
        )
    }
}

impl fmt::Display for CommandLine {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0.join(" "))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    #[error("the command {command} cannot be started: {source}")]
    Start {
        command: CommandLine,
        #[source]
        source: std::io::Error,
    },
    #[error("the command {command} failed:\n{stderr}")]
    Failed {
        command: CommandLine,
        stderr: String,
    },
    #[error("the output of the command {command} is not the expected JSON document: {source}")]
    Decode {
        command: CommandLine,
        #[source]
        source: serde_json::Error,
    },
}

pub fn read_json_output<Document: DeserializeOwned>(
    command: &mut Command,
) -> Result<Document, CommandError> {
    let output = command.output().map_err(|source| CommandError::Start {
        command: CommandLine::from(&*command),
        source,
    })?;
    if !output.status.success() {
        return Err(CommandError::Failed {
            command: CommandLine::from(&*command),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }
    serde_json::from_slice(&output.stdout).map_err(|source| CommandError::Decode {
        command: CommandLine::from(&*command),
        source,
    })
}
