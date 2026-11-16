// SPDX-License-Identifier: GPL-3.0-only
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct CommandResult {
    pub success: bool,
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Output {
    pub rect: OutputSize,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct OutputSize {
    pub width: u32,
    pub height: u32,
}

impl OutputSize {
    pub fn center(self) -> (u32, u32) {
        (self.width / 2, self.height / 2)
    }
}

#[derive(Debug, Deserialize)]
pub struct Node {
    nodes: Vec<Node>,
    floating_nodes: Vec<Node>,
    app_id: Option<String>,
    pid: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SwayWindow {
    pub app_id: String,
    pub pid: u32,
}

impl Node {
    pub fn windows(self) -> Vec<SwayWindow> {
        let mut windows = Vec::new();
        self.collect_windows(&mut windows);
        windows
    }

    fn collect_windows(self, windows: &mut Vec<SwayWindow>) {
        if let (Some(app_id), Some(pid)) = (self.app_id, self.pid) {
            windows.push(SwayWindow { app_id, pid });
        }
        for child in self.nodes.into_iter().chain(self.floating_nodes) {
            child.collect_windows(windows);
        }
    }
}

#[cfg(test)]
mod tests;
