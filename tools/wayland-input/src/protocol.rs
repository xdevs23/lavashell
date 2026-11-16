// SPDX-License-Identifier: GPL-3.0-only
use crate::error::SessionError;
use serde::{Deserialize, Serialize};
use std::io::Write;

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(tag = "command", rename_all = "snake_case", deny_unknown_fields)]
pub enum Request {
    Move { x: u32, y: u32 },
    Key { name: String },
}

impl Request {
    pub fn parse(line: &str) -> Result<Self, SessionError> {
        serde_json::from_str(line).map_err(SessionError::ParseRequest)
    }
}

#[derive(Debug, Serialize)]
#[serde(tag = "reply", rename_all = "snake_case")]
pub enum Reply {
    Ready { pid: u32 },
    Done,
    Failed { message: String },
}

impl Reply {
    pub fn send(&self) -> Result<(), SessionError> {
        let line = serde_json::to_string(self).map_err(SessionError::EncodeReply)?;
        let mut stdout = std::io::stdout().lock();
        writeln!(stdout, "{line}")
            .and_then(|()| stdout.flush())
            .map_err(SessionError::SendReply)
    }
}

#[cfg(test)]
mod tests;
