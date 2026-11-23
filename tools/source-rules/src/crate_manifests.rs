// SPDX-License-Identifier: GPL-3.0-only
// https://doc.rust-lang.org/cargo/reference/manifest.html
use crate::error::Error;
use crate::findings::{Finding, Report};
use crate::metadata::MANIFEST;
use cargo_workspaces::CargoWorkspaces;
use lavashell_enum_values::enum_with_values;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::str::FromStr;

mod cargo_workspaces;
mod crate_rule;
mod vulkan_free;

pub use crate_rule::CrateRule;

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

enum_with_values! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum InheritedTable {
        Package,
        Lints,
    }

    pub const ALL;
}

#[derive(Debug)]
pub struct UnknownInheritedTable;

enum_with_values! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum DependencyTable {
        Dependencies,
        DevDependencies,
        BuildDependencies,
    }

    pub const ALL;
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
    fn table(&self, table: DependencyTable) -> &BTreeMap<String, DependencyEntry> {
        match table {
            DependencyTable::Dependencies => &self.dependencies,
            DependencyTable::DevDependencies => &self.dev_dependencies,
            DependencyTable::BuildDependencies => &self.build_dependencies,
        }
    }

    fn entries(&self) -> impl Iterator<Item = (&String, &DependencyEntry)> {
        (DependencyTable::ALL.into_iter()).flat_map(|table| self.table(table))
    }
}

impl Inheritable {
    fn is_inherited(field: Option<&Self>) -> bool {
        matches!(field, Some(Self::Workspace { workspace: true }))
    }
}

impl InheritedTable {
    pub fn name(self) -> &'static str {
        match self {
            Self::Package => "package",
            Self::Lints => "lints",
        }
    }
}

impl FromStr for InheritedTable {
    type Err = UnknownInheritedTable;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        (Self::ALL.into_iter())
            .find(|table| table.name() == name)
            .ok_or(UnknownInheritedTable)
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
    fn copy_rules(self, workspace: &'a Workspace) -> impl Iterator<Item = CrateRule> + 'a {
        (self.inherited.iter())
            .filter(move |table| workspace.differs_in(self.workspace, **table))
            .map(|table| CrateRule::DifferentWorkspaceTable { table: *table })
    }
}

impl MemberManifest {
    fn dependency_tables(&self) -> impl Iterator<Item = &DependencyTables> {
        std::iter::once(&self.tables).chain(self.target.values())
    }

    fn dependencies_in<'a>(
        &'a self,
        tables: &'a [DependencyTable],
    ) -> impl Iterator<Item = (DependencyTable, &'a str)> {
        tables.iter().flat_map(move |&table| {
            (self.dependency_tables())
                .flat_map(move |tables| tables.table(table).keys())
                .map(move |name| (table, name.as_str()))
        })
    }

    fn rules(&self) -> Vec<CrateRule> {
        let mut rules: Vec<CrateRule> = (self.dependency_tables())
            .flat_map(DependencyTables::entries)
            .filter(|(_, entry)| !entry.is_inherited())
            .map(|(name, _)| CrateRule::OwnDependency { name: name.clone() })
            .collect();
        if !Inheritable::is_inherited(self.package.version.as_ref()) {
            rules.push(CrateRule::OwnVersion);
        }
        if !Inheritable::is_inherited(self.package.edition.as_ref()) {
            rules.push(CrateRule::OwnEdition);
        }
        if !self
            .lints
            .as_ref()
            .is_some_and(|lints| lints.workspace && lints.own.is_empty())
        {
            rules.push(CrateRule::OwnLints);
        }
        rules
    }
}

fn read_toml<Contents: for<'de> Deserialize<'de>>(
    path: &Path,
    malformed: impl FnOnce(PathBuf, toml::de::Error) -> Error,
) -> Result<Contents, Error> {
    let text = std::fs::read_to_string(path).map_err(|source| Error::ReadSource {
        path: path.to_owned(),
        source,
    })?;
    toml::from_str(&text).map_err(|source| malformed(path.to_owned(), source))
}

fn read_manifest<Manifest: for<'de> Deserialize<'de>>(path: &Path) -> Result<Manifest, Error> {
    read_toml(path, |path, source| Error::ReadManifest { path, source })
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
        .map(|(name, _)| CrateRule::InexactWorkspaceDependency { name: name.clone() })
        .chain(
            workspace
                .members
                .iter()
                .filter(|member| !listed_paths.contains(member.as_str()))
                .map(|member| CrateRule::UnlistedMember {
                    member: member.clone(),
                }),
        )
        .chain(root_tables.copy_rules(workspace));
    let mut findings: Vec<Finding> = workspace_rules
        .map(|rule| Finding {
            path: workspace_path.clone(),
            line: None,
            rule: rule.into(),
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
            rule: rule.into(),
        }));
    }
    findings.extend(vulkan_free::vulkan_findings(directory, &members)?);
    Ok(Report {
        findings,
        checked: workspace.members.len() + 1,
    })
}
