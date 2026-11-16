// SPDX-License-Identifier: GPL-3.0-only
use crate::frame::{CapturedFrame, PixelFormat};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::str::FromStr;

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "command", rename_all = "snake_case", deny_unknown_fields)]
pub enum Request {
    CaptureChange { image: PathBuf },
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
    Frame {
        width: u32,
        height: u32,
        stride: u32,
        format: PixelFormat,
        inverted: bool,
    },
    Failed {
        message: String,
    },
}

impl FromStr for Reply {
    type Err = serde_json::Error;

    fn from_str(line: &str) -> Result<Self, Self::Err> {
        serde_json::from_str(line)
    }
}

impl From<CapturedFrame> for Reply {
    fn from(frame: CapturedFrame) -> Self {
        let layout = frame.layout;
        Self::Frame {
            width: layout.width,
            height: layout.height,
            stride: layout.stride,
            format: layout.format,
            inverted: frame.inverted,
        }
    }
}

#[cfg(test)]
mod tests;
