// SPDX-License-Identifier: GPL-3.0-only
use crate::error::HarnessError;
use serde::Deserialize;
use std::collections::HashSet;
use std::hash::Hash;
use std::path::PathBuf;

const DECLARED_SPELLINGS: &str = include_str!("../../../nix/app-declarations.json");

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Host {
    Aosp,
    Checkout,
    Wayland,
}

impl Host {
    const ALL: [Self; 3] = [Self::Aosp, Self::Checkout, Self::Wayland];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LaunchKind {
    Helper,
    Window,
}

impl LaunchKind {
    const ALL: [Self; 2] = [Self::Helper, Self::Window];
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DeclaredSpellings {
    hosts: Vec<Host>,
    launch_kinds: Vec<LaunchKind>,
}

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

pub fn check_declared_spellings() -> Result<(), HarnessError> {
    let declared: DeclaredSpellings =
        serde_json::from_str(DECLARED_SPELLINGS).map_err(HarnessError::UnreadableSpellings)?;
    if !lists_every_value_once(&declared.hosts, &Host::ALL) {
        return Err(HarnessError::DivergedHosts(declared.hosts));
    }
    if !lists_every_value_once(&declared.launch_kinds, &LaunchKind::ALL) {
        return Err(HarnessError::DivergedLaunchKinds(declared.launch_kinds));
    }
    Ok(())
}

fn lists_every_value_once<Value: Eq + Hash>(listed: &[Value], all: &[Value]) -> bool {
    listed.len() == all.len()
        && listed.iter().collect::<HashSet<_>>() == all.iter().collect::<HashSet<_>>()
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
