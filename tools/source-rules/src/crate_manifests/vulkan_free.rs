// SPDX-License-Identifier: GPL-3.0-only
// https://doc.rust-lang.org/cargo/reference/resolver.html#lockfile
use super::{MemberManifest, read_manifest};
use crate::error::Error;
use crate::findings::{Finding, Rule};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

const LOCKFILE: &str = "Cargo.lock";
const VULKAN_FREE_CRATES: [&str; 3] = ["lavashell-core", "lavashell-platform", "lavashell-runtime"];
const VULKAN_CRATES: [&str; 3] = ["ash", "lavashell-vulkan", "lavashell-vulkan-renderer"];

#[derive(Deserialize)]
struct Lockfile {
    package: Vec<LockedPackage>,
}

#[derive(Deserialize)]
struct LockedPackage {
    name: String,
    #[serde(default)]
    dependencies: Vec<String>,
}

type BuildGraph<'a> = BTreeMap<&'a str, BTreeSet<&'a str>>;

pub(super) fn vulkan_findings(
    directory: &Path,
    members: &[(PathBuf, MemberManifest)],
) -> Result<Vec<Finding>, Error> {
    let lockfile: Lockfile = read_manifest(&directory.join(LOCKFILE))?;
    let mut graph = BuildGraph::new();
    for package in &lockfile.package {
        let names =
            (package.dependencies.iter()).filter_map(|entry| entry.split_whitespace().next());
        graph
            .entry(package.name.as_str())
            .or_default()
            .extend(names);
    }
    for (_, manifest) in members {
        let _locked_names_include_dev_dependencies = graph.insert(
            &manifest.package.name,
            manifest.build_dependencies().collect(),
        );
    }
    Ok((members.iter())
        .filter(|(_, manifest)| VULKAN_FREE_CRATES.contains(&manifest.package.name.as_str()))
        .flat_map(|(path, manifest)| {
            vulkan_rules(&graph, manifest).map(move |rule| Finding {
                path: path.clone(),
                line: None,
                rule,
            })
        })
        .collect())
}

fn vulkan_rules<'a>(
    graph: &'a BuildGraph<'a>,
    manifest: &'a MemberManifest,
) -> impl Iterator<Item = Rule> + 'a {
    manifest.build_dependencies().flat_map(move |through| {
        (reached_from(graph, through).into_iter())
            .filter(|name| VULKAN_CRATES.contains(name))
            .map(move |vulkan| Rule::VulkanDependency {
                vulkan: vulkan.to_owned(),
                through: through.to_owned(),
            })
    })
}

fn reached_from<'a>(graph: &BuildGraph<'a>, start: &'a str) -> BTreeSet<&'a str> {
    let mut reached = BTreeSet::from([start]);
    let mut waiting = vec![start];
    while let Some(name) = waiting.pop() {
        for &next in graph.get(name).into_iter().flatten() {
            if reached.insert(next) {
                waiting.push(next);
            }
        }
    }
    reached
}
