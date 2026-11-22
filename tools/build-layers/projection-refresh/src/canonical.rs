// SPDX-License-Identifier: GPL-3.0-only
use crate::error::Error;
use crate::files::write_toml;
use crate::seed::{OriginalContract, SeedManifest, write_stub_targets};
use lavashell_cargo_model::contract::ProjectionContract;
use lavashell_cargo_model::dependency_kind::DependencyKind;
use lavashell_cargo_model::fixed_flag::FixedFlag;
use lavashell_cargo_model::lock::CargoLock;
use lavashell_cargo_model::manifest::{
    BuildScript, DeclaredFeatures, DependencyRequest, ExactVersion, RequestSource, StubLibrary,
    StubManifest, StubPackage,
};
use lavashell_cargo_model::metadata::{CargoMetadata, Package};
use lavashell_cargo_model::package_id::PackageId;
use lavashell_cargo_model::projection::{
    CanonicalCrate, CanonicalPlan, Selection, StubTarget, WorkspaceManifest, WorkspaceSection,
};
use lavashell_cargo_model::unit_graph::{
    Edge, TargetKind, Unit, UnitGraph, UnitIndexError, UnitMode, UnitRecord,
};
use lavashell_plan_check::external_records;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

// https://doc.rust-lang.org/cargo/reference/registry-index.html
const CRATES_IO: &str = "registry+https://github.com/rust-lang/crates.io-index";
// https://doc.rust-lang.org/cargo/reference/resolver.html#resolver-versions
const DEFAULT_RESOLVER: &str = "1";
const SUPPORTED_LOCAL_KINDS: [TargetKind; 5] = [
    TargetKind::Lib,
    TargetKind::Bin,
    TargetKind::Rlib,
    TargetKind::Cdylib,
    TargetKind::Staticlib,
];

pub struct Seed<'seed> {
    pub manifest: &'seed SeedManifest,
    pub original_contracts: &'seed [OriginalContract],
}

pub struct UnitGraphs<'graph> {
    pub build: &'graph UnitGraph,
    pub tests: Option<&'graph UnitGraph>,
}

impl UnitGraphs<'_> {
    fn external_records(&self) -> Result<BTreeSet<UnitRecord>, UnitIndexError> {
        let mut records = external_records(self.build)?;
        if let Some(tests) = self.tests {
            records.extend(external_records(tests)?);
        }
        Ok(records)
    }
}

fn selected_units(graph: &UnitGraph) -> Result<Vec<&Unit>, Error> {
    let mut pending = graph.roots.clone();
    let mut seen = BTreeSet::new();
    while let Some(index) = pending.pop() {
        if seen.insert(index) {
            pending.extend(
                graph
                    .unit(index)?
                    .dependencies
                    .iter()
                    .map(|edge| edge.index),
            );
        }
    }
    if seen.is_empty() {
        return Err(Error::NoUnitsSelected);
    }
    seen.into_iter()
        .map(|index| graph.unit(index).map_err(Error::from))
        .collect()
}

fn member_path(package: &Package, workspace_root: &Path) -> Result<String, Error> {
    let member = package
        .manifest_path
        .parent()
        .and_then(|directory| directory.strip_prefix(workspace_root).ok())
        .ok_or_else(|| Error::ExternalPathPackage {
            package: package.id.clone(),
        })?;
    let contained = member
        .components()
        .all(|component| matches!(component, Component::Normal(_)));
    let text = member.to_string_lossy().into_owned();
    if text.is_empty() || !contained {
        return Err(Error::NonVirtualWorkspace { member: text });
    }
    Ok(text)
}

fn relative_path(from: &str, to: &str) -> String {
    let from: Vec<Component> = Path::new(from).components().collect();
    let to: Vec<Component> = Path::new(to).components().collect();
    let shared = from
        .iter()
        .zip(&to)
        .take_while(|(left, right)| left == right)
        .count();
    let path: PathBuf = std::iter::repeat_n(Component::ParentDir, from.len() - shared)
        .chain(to[shared..].iter().copied())
        .collect();
    path.to_string_lossy().into_owned()
}

struct Packages<'metadata> {
    by_id: BTreeMap<&'metadata PackageId, &'metadata Package>,
}

impl<'metadata> Packages<'metadata> {
    fn get(&self, id: &PackageId) -> Result<&'metadata Package, Error> {
        self.by_id
            .get(id)
            .copied()
            .ok_or_else(|| Error::PackageIdentityMismatch {
                package: id.clone(),
            })
    }
}

fn declared_alias(
    parent: &Package,
    child: &Package,
    unit: &Unit,
    edge: &Edge,
    kind: DependencyKind,
) -> Result<(String, Option<String>), Error> {
    let candidates: BTreeSet<(String, Option<String>)> = parent
        .dependencies
        .iter()
        .filter(|dependency| {
            dependency.name == child.name
                && kind.linked_declaration_kinds().contains(&dependency.kind())
        })
        .filter(|dependency| {
            let expected = dependency.rename.as_ref().unwrap_or(&unit.target.name);
            expected.replace('-', "_") == edge.extern_crate_name
        })
        .map(|dependency| {
            let alias = dependency.rename.as_ref().unwrap_or(&dependency.name);
            (alias.clone(), dependency.source.clone())
        })
        .collect();
    let mut candidates = candidates.into_iter();
    match (candidates.next(), candidates.next()) {
        (Some(candidate), None) => Ok(candidate),
        _ => Err(Error::AmbiguousAlias {
            crate_name: edge.extern_crate_name.clone(),
            package: parent.name.clone(),
        }),
    }
}

fn has_edge_flags(edge: &Edge) -> bool {
    edge.noprelude || edge.nounused || edge.public
}

#[derive(Default)]
struct StubRequirements {
    declarations:
        BTreeMap<PackageId, BTreeMap<DependencyKind, BTreeMap<String, DependencyRequest>>>,
    features: BTreeMap<PackageId, BTreeSet<String>>,
    build_scripts: BTreeSet<PackageId>,
    proc_macros: BTreeSet<PackageId>,
}

fn links_dev_dependencies(unit: &Unit) -> bool {
    match unit.mode {
        UnitMode::Test | UnitMode::DocTest => true,
        UnitMode::Build => unit.target.kind == [TargetKind::Example],
        _ => false,
    }
}

fn units_declaring_beyond_build<'graph>(
    test_units: &[&'graph Unit],
    build_units: &[&Unit],
) -> Vec<&'graph Unit> {
    let built: BTreeSet<&PackageId> = build_units.iter().map(|unit| &unit.pkg_id).collect();
    test_units
        .iter()
        .copied()
        .filter(|unit| links_dev_dependencies(unit) || !built.contains(&unit.pkg_id))
        .collect()
}

impl StubRequirements {
    fn record_features(&mut self, unit: &Unit) {
        self.features
            .entry(unit.pkg_id.clone())
            .or_default()
            .extend(unit.features.iter().cloned());
    }

    fn dependency_kind_of(&mut self, unit: &Unit) -> Result<Option<DependencyKind>, Error> {
        match (unit.mode, unit.target.kind.as_slice()) {
            _ if links_dev_dependencies(unit) => Ok(Some(DependencyKind::Dev)),
            (UnitMode::RunCustomBuild, _) => Ok(None),
            (UnitMode::Build, [TargetKind::CustomBuild]) => {
                let _first_build_script_of_package: bool =
                    self.build_scripts.insert(unit.pkg_id.clone());
                Ok(Some(DependencyKind::Build))
            }
            (UnitMode::Build, [TargetKind::ProcMacro]) => {
                let _first_proc_macro_of_package: bool =
                    self.proc_macros.insert(unit.pkg_id.clone());
                Ok(Some(DependencyKind::Normal))
            }
            (UnitMode::Build, kinds)
                if kinds
                    .iter()
                    .all(|kind| SUPPORTED_LOCAL_KINDS.contains(kind)) =>
            {
                Ok(Some(DependencyKind::Normal))
            }
            (UnitMode::Build, kinds) => Err(Error::UnsupportedTargetKind {
                kinds: kinds.to_vec(),
            }),
            (mode, _) => Err(Error::UnsupportedMode { mode }),
        }
    }

    fn declare(
        &mut self,
        parent: &Package,
        kind: DependencyKind,
        alias: String,
        request: DependencyRequest,
    ) -> Result<(), Error> {
        let dependencies = self
            .declarations
            .entry(parent.id.clone())
            .or_default()
            .entry(kind)
            .or_default();
        match dependencies.get(&alias) {
            Some(declared) if *declared != request => Err(Error::ConflictingRequests {
                package: parent.name.clone(),
                kind,
                alias,
            }),
            _ => {
                let _vacant_slot: Option<DependencyRequest> = dependencies.insert(alias, request);
                Ok(())
            }
        }
    }
}

struct Closure<'plan> {
    graph: &'plan UnitGraph,
    packages: &'plan Packages<'plan>,
    members: &'plan BTreeMap<PackageId, String>,
}

impl Closure<'_> {
    fn dependency_request(
        &self,
        parent: &Package,
        child: &Package,
        child_unit: &Unit,
        edge: &Edge,
        kind: DependencyKind,
    ) -> Result<(String, DependencyRequest), Error> {
        if has_edge_flags(edge) {
            return Err(Error::UnsupportedEdgeFlags {
                parent: parent.name.clone(),
                child: child.name.clone(),
            });
        }
        let (alias, declared) = declared_alias(parent, child, child_unit, edge, kind)?;
        let source = match (&child.source, declared.as_deref()) {
            (None, _) => RequestSource::Path {
                path: relative_path(&self.members[&parent.id], &self.members[&child.id]),
            },
            (Some(_), Some(CRATES_IO)) => RequestSource::Version {
                version: ExactVersion(child.version.clone()),
            },
            (Some(_), Some(declared)) => {
                return Err(Error::UnsupportedSource {
                    declared: declared.to_owned(),
                });
            }
            (Some(_), None) => {
                return Err(Error::UndeclaredSource {
                    package: parent.name.clone(),
                    dependency: child.name.clone(),
                });
            }
        };
        let mut features = child_unit.features.clone();
        features.sort();
        Ok((
            alias,
            DependencyRequest {
                package: child.name.clone(),
                default_features: FixedFlag,
                features,
                source,
            },
        ))
    }

    fn record_declarations(
        &self,
        units: &[&Unit],
        requirements: &mut StubRequirements,
    ) -> Result<(), Error> {
        for unit in units {
            let parent = self.packages.get(&unit.pkg_id)?;
            let dependencies = || {
                unit.dependencies.iter().map(|edge| {
                    let child_unit = self.graph.unit(edge.index)?;
                    Ok::<_, Error>((edge, child_unit, self.packages.get(&child_unit.pkg_id)?))
                })
            };
            if parent.source.is_some() {
                for dependency in dependencies() {
                    let (_edge, _child_unit, child) = dependency?;
                    if child.source.is_none() {
                        return Err(Error::ExternalDependsOnLocal);
                    }
                }
                continue;
            }
            if parent.links.is_some() {
                return Err(Error::LocalLinks {
                    package: parent.name.clone(),
                });
            }
            let Some(kind) = requirements.dependency_kind_of(unit)? else {
                continue;
            };
            for dependency in dependencies() {
                let (edge, child_unit, child) = dependency?;
                if child.id != parent.id {
                    let (alias, request) =
                        self.dependency_request(parent, child, child_unit, edge, kind)?;
                    requirements.declare(parent, kind, alias, request)?;
                }
            }
        }
        Ok(())
    }
}

fn has_path(value: &toml::Value) -> bool {
    match value {
        toml::Value::Table(table) => table_has_path(table),
        toml::Value::Array(items) => items.iter().any(has_path),
        _ => false,
    }
}

fn table_has_path(table: &toml::Table) -> bool {
    matches!(table.get("path"), Some(toml::Value::String(_))) || table.values().any(has_path)
}

fn check_original_contracts(
    contracts: &[OriginalContract],
    active: &BTreeSet<&String>,
    scripted: &BTreeSet<&String>,
) -> Result<(), Error> {
    for contract in contracts
        .iter()
        .filter(|contract| active.contains(&contract.member))
    {
        if contract
            .links
            .as_ref()
            .is_some_and(|links| !links.is_empty())
        {
            return Err(Error::OriginalLinks {
                member: contract.member.clone(),
            });
        }
        if contract.build.0.is_some() && !scripted.contains(&contract.member) {
            return Err(Error::OriginalBuildScript {
                member: contract.member.clone(),
            });
        }
    }
    Ok(())
}

fn canonical_crate(
    package: &Package,
    member: &str,
    requirements: &StubRequirements,
    contract: &ProjectionContract,
) -> CanonicalCrate {
    let scripted = requirements.build_scripts.contains(&package.id);
    let stub = &contract.stub;
    let manifest = StubManifest {
        package: StubPackage {
            name: package.name.clone(),
            version: stub.version.clone(),
            edition: package.edition.clone(),
            autobins: FixedFlag,
            autotests: FixedFlag,
            autoexamples: FixedFlag,
            autobenches: FixedFlag,
            build: BuildScript(scripted.then(|| stub.build_script.clone())),
        },
        lib: StubLibrary {
            path: stub.library.clone(),
            proc_macro: requirements.proc_macros.contains(&package.id),
        },
        features: DeclaredFeatures(
            requirements
                .features
                .get(&package.id)
                .cloned()
                .unwrap_or_default(),
        ),
        dependencies: requirements
            .declarations
            .get(&package.id)
            .cloned()
            .unwrap_or_default(),
    };
    let library = StubTarget {
        path: stub.library.clone(),
        main: false,
    };
    let build_script = scripted.then(|| StubTarget {
        path: stub.build_script.clone(),
        main: true,
    });
    CanonicalCrate {
        member: member.to_owned(),
        manifest,
        targets: std::iter::once(library).chain(build_script).collect(),
    }
}

pub fn canonical_plan(
    seed: &Seed,
    graphs: &UnitGraphs,
    metadata: &CargoMetadata,
    selection: Selection,
    contract: &ProjectionContract,
) -> Result<CanonicalPlan, Error> {
    let packages = Packages {
        by_id: metadata
            .packages
            .iter()
            .map(|package| (&package.id, package))
            .collect(),
    };
    let build_units = selected_units(graphs.build)?;
    let test_units = graphs
        .tests
        .map(selected_units)
        .transpose()?
        .unwrap_or_default();
    let mut members = BTreeMap::new();
    let mut requirements = StubRequirements::default();
    for unit in build_units.iter().chain(&test_units) {
        let package = packages.get(&unit.pkg_id)?;
        if package.source.is_none() {
            let _member_of_same_package: Option<String> = members.insert(
                package.id.clone(),
                member_path(package, &metadata.workspace_root)?,
            );
            requirements.record_features(unit);
        }
    }
    let active: BTreeSet<&String> = members.values().collect();
    if active.len() != members.len() {
        return Err(Error::SharedMember);
    }
    let patched = [&seed.manifest.patch, &seed.manifest.replace]
        .into_iter()
        .flatten()
        .any(table_has_path);
    if patched {
        return Err(Error::PathPatch);
    }
    let closure = |graph| Closure {
        graph,
        packages: &packages,
        members: &members,
    };
    closure(graphs.build).record_declarations(&build_units, &mut requirements)?;
    if let Some(tests) = graphs.tests {
        closure(tests).record_declarations(
            &units_declaring_beyond_build(&test_units, &build_units),
            &mut requirements,
        )?;
    }
    let scripted: BTreeSet<&String> = requirements
        .build_scripts
        .iter()
        .filter_map(|id| members.get(id))
        .collect();
    check_original_contracts(seed.original_contracts, &active, &scripted)?;
    let member_names: BTreeSet<&String> = members
        .keys()
        .map(|id| packages.get(id).map(|package| &package.name))
        .collect::<Result<_, _>>()?;
    if !selection
        .packages
        .iter()
        .all(|name| member_names.contains(name))
    {
        return Err(Error::RootPackageMissing);
    }
    let mut by_member: Vec<(&PackageId, &String)> = members.iter().collect();
    by_member.sort_by_key(|(_, member)| *member);
    let crates = by_member
        .into_iter()
        .map(|(id, member)| {
            packages
                .get(id)
                .map(|package| canonical_crate(package, member, &requirements, contract))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let manifest = WorkspaceManifest {
        profile: seed.manifest.profile.clone(),
        patch: seed.manifest.patch.clone(),
        replace: seed.manifest.replace.clone(),
        workspace: WorkspaceSection {
            resolver: seed
                .manifest
                .workspace
                .resolver
                .clone()
                .unwrap_or_else(|| DEFAULT_RESOLVER.to_owned()),
            members: crates.iter().map(|member| member.member.clone()).collect(),
        },
    };
    Ok(CanonicalPlan {
        schema: contract.schema,
        selection,
        manifest,
        crates,
    })
}

pub fn materialize(
    plan: &CanonicalPlan,
    destination: &Path,
    lock: &CargoLock,
    contract: &ProjectionContract,
) -> Result<(), Error> {
    write_toml(&destination.join("Cargo.toml"), &plan.manifest)?;
    write_toml(&destination.join("Cargo.lock"), lock)?;
    plan.crates.iter().try_for_each(|member| {
        let directory = destination.join(&member.member);
        write_toml(&directory.join("Cargo.toml"), &member.manifest)?;
        write_stub_targets(&directory, &member.targets, &contract.stub)
    })
}

pub fn verify_units(expected: &UnitGraphs, actual: &UnitGraphs) -> Result<(), Error> {
    let wanted = expected.external_records()?;
    let found = actual.external_records()?;
    if wanted == found {
        Ok(())
    } else {
        Err(Error::ExternalContextsChanged {
            missing: wanted.difference(&found).count(),
            extra: found.difference(&wanted).count(),
        })
    }
}

#[cfg(test)]
mod tests;
