// SPDX-License-Identifier: GPL-3.0-only
use super::{Host, LaunchKind};
use serde::Deserialize;
use std::collections::HashSet;
use std::hash::Hash;

const DECLARED_SPELLINGS: &str = include_str!("../../../../nix/app-declarations.json");

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DeclaredSpellings {
    hosts: Vec<Host>,
    launch_kinds: Vec<LaunchKind>,
}

fn read_declared_spellings() -> Result<DeclaredSpellings, serde_json::Error> {
    serde_json::from_str(DECLARED_SPELLINGS)
}

fn lists_every_value_once<Value: Eq + Hash>(listed: &[Value], all: &[Value]) -> bool {
    listed.len() == all.len()
        && listed.iter().collect::<HashSet<_>>() == all.iter().collect::<HashSet<_>>()
}

#[test]
fn nix_lists_every_host_once() -> Result<(), serde_json::Error> {
    let hosts = read_declared_spellings()?.hosts;
    assert!(lists_every_value_once(&hosts, &Host::ALL), "{hosts:?}");
    Ok(())
}

#[test]
fn nix_lists_every_launch_kind_once() -> Result<(), serde_json::Error> {
    let launch_kinds = read_declared_spellings()?.launch_kinds;
    assert!(
        lists_every_value_once(&launch_kinds, &LaunchKind::ALL),
        "{launch_kinds:?}"
    );
    Ok(())
}
