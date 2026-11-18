// SPDX-License-Identifier: GPL-3.0-only
// https://doc.rust-lang.org/cargo/reference/manifest.html
use crate::error::Error;
use crate::findings::{Finding, Report, Rule};
use crate::metadata::MANIFEST;
use cargo_workspaces::CargoWorkspaces;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

mod cargo_workspaces;
mod vulkan_free;

const EXACT_VERSION_PREFIX: char = '=';

#[derive(Deserialize)]
struct WorkspaceManifest {
    workspace: Workspace,
}

#[derive(Deserialize)]
struct Workspace {
    #[serde(default)]
    members: Vec<String>,
    #[serde(default)]
    dependencies: BTreeMap<String, DependencyEntry>,
    package: Option<WorkspacePackage>,
    lints: Option<BTreeMap<String, BTreeMap<String, LintSetting>>>,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
enum InheritedTable {
    Package,
    Lints,
}

#[derive(Clone, Copy)]
struct RootTables<'a> {
    workspace: &'a Workspace,
    inherited: &'a [InheritedTable],
}

#[derive(Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct WorkspacePackage {
    version: Option<String>,
    edition: Option<String>,
    license: Option<String>,
}

#[derive(Deserialize, PartialEq)]
#[serde(untagged)]
enum LintSetting {
    Level(LintLevel),
    Detailed(DetailedLintSetting),
}

#[derive(Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct DetailedLintSetting {
    level: LintLevel,
    priority: Option<i64>,
}

#[derive(Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
enum LintLevel {
    Allow,
    Warn,
    Deny,
    Forbid,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum DependencyEntry {
    Version(String),
    Detailed(DetailedDependency),
}

#[derive(Deserialize)]
struct DetailedDependency {
    version: Option<String>,
    path: Option<String>,
    #[serde(default)]
    workspace: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
struct DependencyTables {
    #[serde(default)]
    dependencies: BTreeMap<String, DependencyEntry>,
    #[serde(default)]
    dev_dependencies: BTreeMap<String, DependencyEntry>,
    #[serde(default)]
    build_dependencies: BTreeMap<String, DependencyEntry>,
}

#[derive(Deserialize)]
struct MemberManifest {
    package: MemberPackage,
    lints: Option<Lints>,
    #[serde(flatten)]
    tables: DependencyTables,
    #[serde(default)]
    target: BTreeMap<String, DependencyTables>,
}

#[derive(Deserialize)]
struct MemberPackage {
    name: String,
    version: Option<Inheritable>,
    edition: Option<Inheritable>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Inheritable {
    Workspace { workspace: bool },
    Own(serde::de::IgnoredAny),
}

#[derive(Deserialize)]
struct Lints {
    #[serde(default)]
    workspace: bool,
    #[serde(flatten)]
    own: toml::Table,
}

impl DependencyEntry {
    fn is_exact_or_path(&self) -> bool {
        match self {
            Self::Version(version) => version.starts_with(EXACT_VERSION_PREFIX),
            Self::Detailed(detailed) => {
                detailed.path.is_some()
                    || detailed
                        .version
                        .as_ref()
                        .is_some_and(|version| version.starts_with(EXACT_VERSION_PREFIX))
            }
        }
    }

    fn is_inherited(&self) -> bool {
        matches!(
            self,
            Self::Detailed(DetailedDependency {
                workspace: true,
                ..
            })
        )
    }

    fn path(&self) -> Option<&str> {
        match self {
            Self::Version(_) => None,
            Self::Detailed(detailed) => detailed.path.as_deref(),
        }
    }
}

impl DependencyTables {
    fn entries(&self) -> impl Iterator<Item = (&String, &DependencyEntry)> {
        self.dependencies
            .iter()
            .chain(&self.dev_dependencies)
            .chain(&self.build_dependencies)
    }

    fn build_names(&self) -> impl Iterator<Item = &str> {
        (self.dependencies.keys())
            .chain(self.build_dependencies.keys())
            .map(String::as_str)
    }
}

impl Inheritable {
    fn is_inherited(field: Option<&Self>) -> bool {
        matches!(field, Some(Self::Workspace { workspace: true }))
    }
}

impl InheritedTable {
    fn name(self) -> &'static str {
        match self {
            Self::Package => "package",
            Self::Lints => "lints",
        }
    }
}

impl Workspace {
    fn differs_in(&self, root: &Self, table: InheritedTable) -> bool {
        match table {
            InheritedTable::Package => self.package != root.package,
            InheritedTable::Lints => self.lints != root.lints,
        }
    }
}

impl<'a> RootTables<'a> {
    fn copy_rules(self, workspace: &'a Workspace) -> impl Iterator<Item = Rule> + 'a {
        (self.inherited.iter())
            .filter(move |table| workspace.differs_in(self.workspace, **table))
            .map(|table| Rule::DifferentWorkspaceTable {
                table: table.name(),
            })
    }
}

impl MemberManifest {
    fn build_dependencies(&self) -> impl Iterator<Item = &str> {
        std::iter::once(&self.tables)
            .chain(self.target.values())
            .flat_map(DependencyTables::build_names)
    }

    fn rules(&self) -> Vec<Rule> {
        let mut rules: Vec<Rule> = std::iter::once(&self.tables)
            .chain(self.target.values())
            .flat_map(DependencyTables::entries)
            .filter(|(_, entry)| !entry.is_inherited())
            .map(|(name, _)| Rule::OwnDependency { name: name.clone() })
            .collect();
        if !Inheritable::is_inherited(self.package.version.as_ref()) {
            rules.push(Rule::OwnVersion);
        }
        if !Inheritable::is_inherited(self.package.edition.as_ref()) {
            rules.push(Rule::OwnEdition);
        }
        if !self
            .lints
            .as_ref()
            .is_some_and(|lints| lints.workspace && lints.own.is_empty())
        {
            rules.push(Rule::OwnLints);
        }
        rules
    }
}

fn read_manifest<Manifest: for<'de> Deserialize<'de>>(path: &Path) -> Result<Manifest, Error> {
    let text = std::fs::read_to_string(path).map_err(|source| Error::ReadSource {
        path: path.to_owned(),
        source,
    })?;
    toml::from_str(&text).map_err(|source| Error::ReadManifest {
        path: path.to_owned(),
        source,
    })
}

pub fn crate_report(root: &Path, nested_directories: &[PathBuf]) -> Result<Report, Error> {
    let listing = CargoWorkspaces::read(root)?;
    let root_workspace = read_workspace(root)?;
    let root_tables = RootTables {
        workspace: &root_workspace,
        inherited: &listing.inherited_tables,
    };
    let mut report = workspace_report(root, &root_workspace, root_tables)?;
    for directory in nested_directories {
        let nested_report = workspace_report(directory, &read_workspace(directory)?, root_tables)?;
        report.findings.extend(nested_report.findings);
        report.checked += nested_report.checked;
    }
    report
        .findings
        .extend(listing.findings(root, nested_directories));
    Ok(report)
}

fn read_workspace(directory: &Path) -> Result<Workspace, Error> {
    Ok(read_manifest::<WorkspaceManifest>(&directory.join(MANIFEST))?.workspace)
}

fn workspace_report(
    directory: &Path,
    workspace: &Workspace,
    root_tables: RootTables<'_>,
) -> Result<Report, Error> {
    let workspace_path = directory.join(MANIFEST);
    let listed_paths: BTreeSet<&str> = workspace
        .dependencies
        .values()
        .filter_map(DependencyEntry::path)
        .collect();
    let workspace_rules = workspace
        .dependencies
        .iter()
        .filter(|(_, entry)| !entry.is_exact_or_path())
        .map(|(name, _)| Rule::InexactWorkspaceDependency { name: name.clone() })
        .chain(
            workspace
                .members
                .iter()
                .filter(|member| !listed_paths.contains(member.as_str()))
                .map(|member| Rule::UnlistedMember {
                    member: member.clone(),
                }),
        )
        .chain(root_tables.copy_rules(workspace));
    let mut findings: Vec<Finding> = workspace_rules
        .map(|rule| Finding {
            path: workspace_path.clone(),
            line: None,
            rule,
        })
        .collect();
    let members = (workspace.members.iter())
        .map(|member| {
            let path = directory.join(member).join(MANIFEST);
            read_manifest(&path).map(|manifest: MemberManifest| (path, manifest))
        })
        .collect::<Result<Vec<(PathBuf, MemberManifest)>, Error>>()?;
    for (path, manifest) in &members {
        findings.extend(manifest.rules().into_iter().map(|rule| Finding {
            path: path.clone(),
            line: None,
            rule,
        }));
    }
    findings.extend(vulkan_free::vulkan_findings(directory, &members)?);
    Ok(Report {
        findings,
        checked: workspace.members.len() + 1,
    })
}
