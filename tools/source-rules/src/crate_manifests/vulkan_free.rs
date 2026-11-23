// SPDX-License-Identifier: GPL-3.0-only
// https://doc.rust-lang.org/cargo/reference/resolver.html#lockfile
use super::{CrateRule, DependencyTable, MemberManifest, read_manifest};
use crate::error::Error;
use crate::findings::Finding;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

const LOCKFILE: &str = "Cargo.lock";
const VULKAN_FREE_CRATES: [(&str, VulkanFreeTargets); 4] = [
    ("lavashell-core", VulkanFreeTargets::Library),
    ("lavashell-platform", VulkanFreeTargets::Library),
    ("lavashell-runtime", VulkanFreeTargets::LibraryAndTests),
    (
        "lavashell-test-platform",
        VulkanFreeTargets::LibraryAndTests,
    ),
];
const VULKAN_CRATES: [&str; 3] = ["ash", "lavashell-vulkan", "lavashell-vulkan-renderer"];

#[derive(Clone, Copy)]
enum VulkanFreeTargets {
    Library,
    LibraryAndTests,
}

impl VulkanFreeTargets {
    fn dependency_tables(self) -> &'static [DependencyTable] {
        match self {
            Self::Library => &[
                DependencyTable::Dependencies,
                DependencyTable::BuildDependencies,
            ],
            Self::LibraryAndTests => &DependencyTable::ALL,
        }
    }
}

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
    let library = VulkanFreeTargets::Library.dependency_tables();
    for (_, manifest) in members {
        let _locked_names_include_dev_dependencies = graph.insert(
            &manifest.package.name,
            (manifest.dependencies_in(library))
                .map(|(_, name)| name)
                .collect(),
        );
    }
    Ok((members.iter())
        .filter_map(|(path, manifest)| {
            (VULKAN_FREE_CRATES.iter())
                .find(|(name, _)| *name == manifest.package.name)
                .map(|&(_, targets)| (path, manifest, targets))
        })
        .flat_map(|(path, manifest, targets)| {
            vulkan_rules(&graph, manifest, targets).map(move |rule| Finding {
                path: path.clone(),
                line: None,
                rule: rule.into(),
            })
        })
        .collect())
}

fn vulkan_rules<'a>(
    graph: &'a BuildGraph<'a>,
    manifest: &'a MemberManifest,
    targets: VulkanFreeTargets,
) -> impl Iterator<Item = CrateRule> + 'a {
    (manifest.dependencies_in(targets.dependency_tables())).flat_map(move |(table, through)| {
        (reached_from(graph, through).into_iter())
            .filter(|name| VULKAN_CRATES.contains(name))
            .map(move |vulkan| CrateRule::VulkanDependency {
                vulkan: vulkan.to_owned(),
                through: through.to_owned(),
                table,
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
