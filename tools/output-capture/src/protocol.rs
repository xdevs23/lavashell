// SPDX-License-Identifier: GPL-3.0-only
use crate::error::SessionError;
use crate::frame::{CapturedFrame, PixelFormat};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::PathBuf;

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(tag = "command", rename_all = "snake_case", deny_unknown_fields)]
pub enum Request {
    CaptureChange { pixels: PathBuf },
}

impl Request {
    pub fn parse(line: &str) -> Result<Self, SessionError> {
        serde_json::from_str(line).map_err(SessionError::ParseRequest)
    }
}

#[derive(Debug, Serialize)]
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
