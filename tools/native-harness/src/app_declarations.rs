// SPDX-License-Identifier: GPL-3.0-only
use crate::app_vocabulary::{Host, LaunchKind};
use crate::error::HarnessError;
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Deserialize)]
struct AppDeclaration {
    program: PathBuf,
    meta: AppMeta,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AppMeta {
    lavashell_host: Host,
    lavashell_launch: Option<Launch>,
}

#[derive(Debug, Deserialize)]
pub struct Launch {
    pub kind: LaunchKind,
    pub arguments: Vec<String>,
}

#[derive(Debug)]
pub struct WaylandApp {
    pub name: String,
    pub program: PathBuf,
    pub launch: Launch,
}

pub fn read_wayland_app(
    name: String,
    declaration: serde_json::Value,
) -> Result<Option<WaylandApp>, HarnessError> {
    let declaration: AppDeclaration = match serde_json::from_value(declaration) {
        Ok(declaration) => declaration,
        Err(source) => return Err(HarnessError::InvalidDeclaration { app: name, source }),
    };
    match (
        declaration.meta.lavashell_host,
        declaration.meta.lavashell_launch,
    ) {
        (Host::Wayland, Some(launch)) => Ok(Some(WaylandApp {
            name,
            program: declaration.program,
            launch,
        })),
        (Host::Wayland, None) => Err(HarnessError::MissingLaunch { app: name }),
        (Host::Aosp | Host::Checkout, _) => Ok(None),
    }
}
