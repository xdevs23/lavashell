// SPDX-License-Identifier: GPL-3.0-only
use serde::{Deserialize, Serialize};
use std::str::FromStr;

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "command", rename_all = "snake_case", deny_unknown_fields)]
pub enum Request {
    Move { x: u32, y: u32 },
    Key { name: String },
}

impl FromStr for Request {
    type Err = serde_json::Error;

    fn from_str(line: &str) -> Result<Self, Self::Err> {
        serde_json::from_str(line)
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "reply", rename_all = "snake_case")]
pub enum Reply {
    Ready { pid: u32 },
    Done,
    Failed { message: String },
}

impl FromStr for Reply {
    type Err = serde_json::Error;

    fn from_str(line: &str) -> Result<Self, Self::Err> {
        serde_json::from_str(line)
    }
}

#[cfg(test)]
mod tests;
