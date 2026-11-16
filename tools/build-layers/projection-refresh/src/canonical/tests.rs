// SPDX-License-Identifier: GPL-3.0-only
use super::{CRATES_IO, Seed, canonical_plan, materialize, verify_units};
use crate::error::Error;
use crate::files::read_toml;
use crate::seed::{OriginalContract, SeedManifest};
use lavashell_cargo_model::contract::{ProjectionContract, StubLayout};
use lavashell_cargo_model::dependency_kind::DependencyKind;
use lavashell_cargo_model::lock::{CargoLock, LockReference, LockedPackage};
use lavashell_cargo_model::manifest::{
    BuildScript, DependencyRequest, ExactVersion, RequestSource, StubManifest,
};
use lavashell_cargo_model::metadata::CargoMetadata;
use lavashell_cargo_model::projection::{
    CanonicalPlan, Layer, Selection, StubTarget, WorkspaceManifest,
};
use lavashell_cargo_model::unit_graph::UnitGraph;
use serde_json::{Value, json};
use std::collections::BTreeMap;

const ROOT: &str = "/fixture";
const TARGET: &str = "x86_64-unknown-linux-gnu";

#[derive(Debug, thiserror::Error)]
enum TestError {
    #[error(transparent)]
    Plan(#[from] Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Toml(#[from] toml::de::Error),
    #[error(transparent)]
    Directory(#[from] std::io::Error),
}

fn contract() -> ProjectionContract {
    ProjectionContract {
        schema: 1,
        profile: "packaged".to_owned(),
        target: TARGET.to_owned(),
        projection_directory: "nix/cargo-projections".to_owned(),
        engine_root: "bevy".to_owned(),
        native_directory: "examples/support".to_owned(),
        layers: vec![Layer::Engine, Layer::Registry],
        companions: Vec::new(),
        packaged_crates: BTreeMap::new(),
        stub: StubLayout {
            version: "0.0.0".to_owned(),
            library: "src/lib.rs".to_owned(),
            build_script: "build.rs".to_owned(),
            main_source: "fn main() {}".to_owned(),
            library_source: String::new(),
        },
    }
}

#[derive(Clone)]
struct Package {
    id: String,
    name: &'static str,
    version: &'static str,
    source: Option<&'static str>,
    manifest_path: String,
    dependencies: Vec<Value>,
    links: Option<&'static str>,
}

fn package(name: &'static str, local: bool) -> Package {
    versioned(name, local, "1.0.0")
}

fn versioned(name: &'static str, local: bool, version: &'static str) -> Package {
    let source = (!local).then_some(CRATES_IO);
    let id = match source {
        Some(source) => format!("{source}#{name}@{version}"),
        None => format!("path+file://{ROOT}/{name}#{name}@{version}"),
    };
    Package {
        id,
        name,
        version,
        source,
        manifest_path: format!("{ROOT}/{name}/Cargo.toml"),
        dependencies: Vec::new(),
        links: None,
    }
}

fn declaration(
    name: &str,
    rename: Option<&str>,
    kind: Option<&str>,
    source: Option<&str>,
) -> Value {
    json!({ "name": name, "rename": rename, "kind": kind, "source": source })
}

fn registry_declaration(name: &str) -> Value {
    declaration(name, None, None, Some(CRATES_IO))
}

impl Package {
    fn json(&self) -> Value {
        json!({
            "id": self.id, "name": self.name, "version": self.version, "edition": "2024",
            "source": self.source, "manifest_path": self.manifest_path,
            "dependencies": self.dependencies, "links": self.links
        })
    }
}

#[derive(Clone, Copy)]
enum Role {
    Target,
    Host,
}

fn unit(
    package: &Package,
    edges: &[(usize, &str)],
    features: &[&str],
    role: Role,
    kind: &str,
    mode: &str,
) -> Value {
    let crate_type = if kind == "proc-macro" {
        "proc-macro"
    } else {
        "lib"
    };
    let (opt_level, platform) = match role {
        Role::Target => ("2", Some(TARGET)),
        Role::Host => ("0", None),
    };
    json!({
        "pkg_id": package.id,
        "target": {
            "name": package.name.replace('-', "_"), "kind": [kind], "crate_types": [crate_type]
        },
        "profile": {
            "name": "packaged", "opt_level": opt_level, "lto": "false", "codegen_backend": null,
            "codegen_units": null, "debuginfo": 0, "debug_assertions": false,
            "overflow_checks": false, "rpath": false, "incremental": false, "panic": "unwind",
            "split_debuginfo": null, "strip": { "resolved": { "Named": "debuginfo" } }
        },
        "platform": platform,
        "mode": mode,
        "features": features,
        "dependencies": edges.iter().map(|(index, name)| json!({
            "index": index, "extern_crate_name": name.replace('-', "_"),
            "noprelude": false, "nounused": false, "public": false
        })).collect::<Vec<_>>()
    })
}

fn library(package: &Package, edges: &[(usize, &str)], features: &[&str]) -> Value {
    unit(package, edges, features, Role::Target, "lib", "build")
}

struct Fixture {
    packages: Vec<Package>,
    units: Vec<Value>,
    roots: Vec<usize>,
    selection: Selection,
    manifest: toml::Table,
    contracts: Vec<OriginalContract>,
}

impl Fixture {
    fn new() -> Result<Self, TestError> {
        let mut app = package("app", true);
        app.dependencies.push(registry_declaration("dep"));
        let dep = package("dep", false);
        Ok(Self {
            units: vec![
                library(&app, &[(1, "dep")], &[]),
                library(&dep, &[], &["std", "default"]),
            ],
            packages: vec![app, dep],
            roots: vec![0],
            selection: Selection {
                target: TARGET.to_owned(),
                profile: "packaged".to_owned(),
                packages: vec!["app".to_owned()],
                features: Vec::new(),
            },
            manifest: toml::from_str(
                "[workspace]\nresolver = \"3\"\nmembers = [\"app\"]\n\n\
                 [profile.packaged]\ninherits = \"dev\"\nopt-level = 2\n",
            )?,
            contracts: Vec::new(),
        })
    }

    fn graph(&self) -> Result<UnitGraph, TestError> {
        Ok(serde_json::from_value(
            json!({ "units": self.units, "roots": self.roots }),
        )?)
    }

    fn plan(&self) -> Result<CanonicalPlan, TestError> {
        let metadata: CargoMetadata = serde_json::from_value(json!({
            "workspace_root": ROOT,
            "packages": self.packages.iter().map(Package::json).collect::<Vec<_>>()
        }))?;
        let manifest: SeedManifest = self.manifest.clone().try_into()?;
        Ok(canonical_plan(
            &Seed {
                manifest: &manifest,
                original_contracts: &self.contracts,
            },
            &self.graph()?,
            &metadata,
            self.selection.clone(),
            &contract(),
        )?)
    }

    fn app(&self) -> &Package {
        &self.packages[0]
    }

    fn dep(&self) -> &Package {
        &self.packages[1]
    }
}

fn member<'plan>(plan: &'plan CanonicalPlan, name: &str) -> Option<&'plan StubManifest> {
    plan.crates
        .iter()
        .find(|member| member.member == name)
        .map(|member| &member.manifest)
}

fn request<'plan>(
    plan: &'plan CanonicalPlan,
    name: &str,
    kind: DependencyKind,
    alias: &str,
) -> Option<&'plan DependencyRequest> {
    member(plan, name)?.dependencies.get(&kind)?.get(alias)
}

fn exact(version: &str) -> RequestSource {
    RequestSource::Version {
        version: ExactVersion(version.to_owned()),
    }
}

fn contract_for(member: &str, links: Option<&str>, build: Option<&str>) -> OriginalContract {
    OriginalContract {
        member: member.to_owned(),
        links: links.map(str::to_owned),
        build: BuildScript(build.map(str::to_owned)),
    }
}

#[test]
fn checks_the_original_engine_contract_of_active_members_only() -> Result<(), TestError> {
    let mut fixture = Fixture::new()?;
    fixture.contracts = vec![contract_for("inactive", Some("native"), Some("build.rs"))];
    let _accepted_plan: CanonicalPlan = fixture.plan()?;
    fixture.contracts = vec![contract_for("app", Some("native"), None)];
    assert!(
        matches!(fixture.plan(), Err(TestError::Plan(Error::OriginalLinks { member })) if member == "app")
    );
    fixture.contracts = vec![contract_for("app", None, Some("build.rs"))];
    assert!(matches!(
        fixture.plan(),
        Err(TestError::Plan(Error::OriginalBuildScript { member })) if member == "app"
    ));
    Ok(())
}

#[test]
fn a_build_script_and_its_local_build_time_crate_enter_the_plan() -> Result<(), TestError> {
    let mut fixture = Fixture::new()?;
    let mut keys = package("keys", true);
    keys.dependencies.push(registry_declaration("dep"));
    fixture.packages[0]
        .dependencies
        .push(declaration("keys", None, Some("build"), None));
    let (app, dep) = (fixture.app().clone(), fixture.dep().clone());
    fixture.units.extend([
        unit(&dep, &[], &["host"], Role::Host, "lib", "build"),
        unit(&keys, &[(2, "dep")], &[], Role::Host, "lib", "build"),
        unit(
            &app,
            &[(3, "keys")],
            &[],
            Role::Host,
            "custom-build",
            "build",
        ),
        unit(
            &app,
            &[(4, "build_script_build")],
            &[],
            Role::Target,
            "custom-build",
            "run-custom-build",
        ),
    ]);
    fixture.packages.push(keys);
    fixture.units[0] = library(&app, &[(1, "dep"), (5, "build_script_build")], &[]);
    fixture.contracts = vec![
        contract_for("app", None, Some("build.rs")),
        contract_for("keys", None, None),
    ];
    let plan = fixture.plan()?;
    let app_manifest = member(&plan, "app");
    assert_eq!(
        app_manifest.map(|manifest| &manifest.package.build),
        Some(&BuildScript(Some("build.rs".to_owned())))
    );
    let app_targets = plan.crates.iter().find(|member| member.member == "app");
    assert!(
        app_targets.is_some_and(|member| member.targets.contains(&StubTarget {
            path: "build.rs".to_owned(),
            main: true
        }))
    );
    assert_eq!(
        request(&plan, "app", DependencyKind::Build, "keys").map(|request| &request.source),
        Some(&RequestSource::Path {
            path: "../keys".to_owned()
        })
    );
    assert_eq!(
        request(&plan, "keys", DependencyKind::Normal, "dep").map(|request| &request.features),
        Some(&vec!["host".to_owned()])
    );
    assert_eq!(
        member(&plan, "keys").map(|manifest| &manifest.package.build),
        Some(&BuildScript(None))
    );
    Ok(())
}

#[test]
fn unused_declarations_and_members_stay_out_of_the_plan() -> Result<(), TestError> {
    let mut fixture = Fixture::new()?;
    let before = fixture.plan()?;
    fixture.packages[0]
        .dependencies
        .push(declaration("platform-only", None, None, None));
    fixture.packages.push(package("platform-only", true));
    let after = fixture.plan()?;
    assert_eq!(before, after);
    assert_eq!(
        after
            .crates
            .iter()
            .map(|member| member.member.as_str())
            .collect::<Vec<_>>(),
        ["app"]
    );
    Ok(())
}

#[test]
fn equivalent_requests_become_one_dependency() -> Result<(), TestError> {
    let mut fixture = Fixture::new()?;
    let before = fixture.plan()?;
    fixture.packages[0]
        .dependencies
        .push(registry_declaration("dep"));
    assert_eq!(before, fixture.plan()?);
    let dependency = request(&before, "app", DependencyKind::Normal, "dep");
    assert_eq!(
        dependency.map(|request| &request.source),
        Some(&exact("1.0.0"))
    );
    assert_eq!(
        dependency.map(|request| &request.features),
        Some(&vec!["default".to_owned(), "std".to_owned()])
    );
    Ok(())
}

#[test]
fn version_feature_profile_and_patch_changes_change_the_plan() -> Result<(), TestError> {
    let mut fixture = Fixture::new()?;
    let baseline = fixture.plan()?;
    let dep = fixture.dep().clone();
    fixture.units[1] = library(&dep, &[], &["std", "default", "changed"]);
    assert_ne!(baseline, fixture.plan()?);
    fixture.units[1] = library(&dep, &[], &["std", "default"]);
    fixture.packages[1].version = "2.0.0";
    assert_ne!(baseline, fixture.plan()?);
    fixture.packages[1].version = "1.0.0";
    fixture.manifest = toml::from_str(
        "[workspace]\nresolver = \"3\"\nmembers = [\"app\"]\n\n\
         [profile.packaged]\ninherits = \"dev\"\nopt-level = 3\n\n\
         [patch.crates-io.patched]\ngit = \"https://example.invalid/repo\"\n",
    )?;
    let patched = fixture.plan()?;
    assert_ne!(baseline, patched);
    assert_eq!(
        patched.manifest.patch,
        fixture
            .manifest
            .get("patch")
            .and_then(toml::Value::as_table)
            .cloned()
    );
    Ok(())
}

#[test]
fn aliases_and_two_exact_versions_stay_distinct() -> Result<(), TestError> {
    let mut fixture = Fixture::new()?;
    fixture.packages[0].dependencies = vec![
        declaration("dep", Some("old"), None, Some(CRATES_IO)),
        declaration("dep", Some("new"), None, Some(CRATES_IO)),
    ];
    let new = versioned("dep", false, "2.0.0");
    fixture.units.push(library(&new, &[], &[]));
    fixture.packages.push(new);
    let app = fixture.app().clone();
    fixture.units[0] = library(&app, &[(1, "old"), (2, "new")], &[]);
    let plan = fixture.plan()?;
    for (alias, version) in [("old", "1.0.0"), ("new", "2.0.0")] {
        assert_eq!(
            request(&plan, "app", DependencyKind::Normal, alias).map(|request| &request.source),
            Some(&exact(version))
        );
    }
    Ok(())
}

#[test]
fn host_build_dependencies_and_proc_macros_keep_their_roles() -> Result<(), TestError> {
    let mut fixture = Fixture::new()?;
    fixture.packages[0].dependencies.extend([
        declaration("dep", None, Some("build"), Some(CRATES_IO)),
        registry_declaration("derive"),
    ]);
    let derive = package("derive", false);
    let (app, dep) = (fixture.app().clone(), fixture.dep().clone());
    fixture.units.extend([
        unit(&dep, &[], &["host"], Role::Host, "lib", "build"),
        unit(&derive, &[], &[], Role::Host, "proc-macro", "build"),
        unit(
            &app,
            &[(2, "dep")],
            &[],
            Role::Host,
            "custom-build",
            "build",
        ),
    ]);
    fixture.packages.push(derive);
    fixture.units[0] = library(
        &app,
        &[(1, "dep"), (3, "derive"), (4, "build_script_build")],
        &[],
    );
    let plan = fixture.plan()?;
    assert!(request(&plan, "app", DependencyKind::Normal, "derive").is_some());
    assert_eq!(
        request(&plan, "app", DependencyKind::Build, "dep").map(|request| &request.features),
        Some(&vec!["host".to_owned()])
    );
    assert_eq!(
        request(&plan, "app", DependencyKind::Normal, "dep").map(|request| &request.features),
        Some(&vec!["default".to_owned(), "std".to_owned()])
    );
    assert_eq!(
        member(&plan, "app").map(|manifest| &manifest.package.build),
        Some(&BuildScript(Some("build.rs".to_owned())))
    );
    Ok(())
}

#[test]
fn conflicting_requests_in_one_manifest_slot_fail() -> Result<(), TestError> {
    let mut fixture = Fixture::new()?;
    let (app, dep) = (fixture.app().clone(), fixture.dep().clone());
    fixture.units.extend([
        unit(&dep, &[], &["host"], Role::Host, "lib", "build"),
        unit(&app, &[(2, "dep")], &[], Role::Host, "lib", "build"),
    ]);
    fixture.roots.push(3);
    assert!(matches!(
        fixture.plan(),
        Err(TestError::Plan(Error::ConflictingRequests { package, kind: DependencyKind::Normal, alias }))
            if package == "app" && alias == "dep"
    ));
    Ok(())
}

#[test]
fn local_dependencies_are_stored_as_relative_paths() -> Result<(), TestError> {
    let mut fixture = Fixture::new()?;
    let child = package("child", true);
    fixture.packages[0]
        .dependencies
        .push(declaration("child", None, None, None));
    fixture.units.push(library(&child, &[], &[]));
    fixture.packages.push(child);
    let app = fixture.app().clone();
    fixture.units[0] = library(&app, &[(1, "dep"), (2, "child")], &[]);
    let plan = fixture.plan()?;
    assert!(!serde_json::to_string(&plan)?.contains(ROOT));
    assert_eq!(
        request(&plan, "app", DependencyKind::Normal, "child").map(|request| &request.source),
        Some(&RequestSource::Path {
            path: "../child".to_owned()
        })
    );
    Ok(())
}

#[test]
fn unreachable_units_stay_out_of_the_plan() -> Result<(), TestError> {
    let mut fixture = Fixture::new()?;
    let foreign = package("foreign", true);
    fixture.units.push(library(&foreign, &[], &[]));
    fixture.packages.push(foreign);
    assert_eq!(fixture.plan()?.crates.len(), 1);
    Ok(())
}

#[test]
fn a_registry_dependency_patched_to_git_is_requested_by_its_exact_version() -> Result<(), TestError>
{
    let mut fixture = Fixture::new()?;
    let git = "git+https://example.invalid/repo?rev=0123abc#0123abc";
    fixture.manifest = toml::from_str(
        "[workspace]\nresolver = \"3\"\nmembers = [\"app\"]\n\n\
         [patch.crates-io.dep]\ngit = \"https://example.invalid/repo\"\nrev = \"0123abc\"\n",
    )?;
    fixture.packages[1].source = Some(git);
    fixture.packages[1].id = format!("{git}#dep@1.0.0");
    let dep = fixture.dep().clone();
    fixture.units[1] = library(&dep, &[], &["std", "default"]);
    let plan = fixture.plan()?;
    assert_eq!(
        request(&plan, "app", DependencyKind::Normal, "dep").map(|request| &request.source),
        Some(&exact("1.0.0"))
    );
    assert_eq!(
        lavashell_plan_check::external_records(&fixture.graph()?)
            .map_err(Error::from)?
            .len(),
        1
    );
    Ok(())
}

#[test]
fn unsupported_sources_links_and_outside_paths_fail() -> Result<(), TestError> {
    let git = "git+https://example.invalid/repo";
    let mut fixture = Fixture::new()?;
    fixture.packages[1].source = Some(git);
    fixture.packages[0].dependencies = vec![declaration("dep", None, None, Some(git))];
    assert!(matches!(
        fixture.plan(),
        Err(TestError::Plan(Error::UnsupportedSource { declared })) if declared == git
    ));
    let mut fixture = Fixture::new()?;
    fixture.packages[0].links = Some("native-library");
    assert!(
        matches!(fixture.plan(), Err(TestError::Plan(Error::LocalLinks { package })) if package == "app")
    );
    let mut fixture = Fixture::new()?;
    fixture.packages[0].manifest_path = "/outside/Cargo.toml".to_owned();
    assert!(matches!(
        fixture.plan(),
        Err(TestError::Plan(Error::ExternalPathPackage { .. }))
    ));
    Ok(())
}

#[test]
fn a_materialized_plan_reads_back_as_written() -> Result<(), TestError> {
    let plan = Fixture::new()?.plan()?;
    let lock = CargoLock {
        version: 4,
        package: vec![LockedPackage {
            name: "app".to_owned(),
            version: "0.0.0".to_owned(),
            source: None,
            checksum: None,
            dependencies: vec![LockReference::from("dep".to_owned())],
        }],
    };
    let directory = tempfile::tempdir()?;
    let destination = directory.path().join("plan");
    materialize(&plan, &destination, &lock, &contract())?;
    assert_eq!(
        read_toml::<WorkspaceManifest>(&destination.join("Cargo.toml"))?,
        plan.manifest
    );
    assert_eq!(
        read_toml::<CargoLock>(&destination.join("Cargo.lock"))?,
        lock
    );
    for member in &plan.crates {
        let written: StubManifest =
            read_toml(&destination.join(&member.member).join("Cargo.toml"))?;
        assert_eq!(written, member.manifest);
    }
    Ok(())
}

#[test]
fn path_patches_and_dependency_edge_flags_fail() -> Result<(), TestError> {
    let mut fixture = Fixture::new()?;
    fixture.manifest = toml::from_str(
        "[workspace]\nmembers = [\"app\"]\n\n[patch.crates-io.dep]\npath = \"patched\"\n",
    )?;
    assert!(matches!(
        fixture.plan(),
        Err(TestError::Plan(Error::PathPatch))
    ));
    let mut fixture = Fixture::new()?;
    fixture.units[0]["dependencies"][0]["public"] = json!(true);
    assert!(matches!(
        fixture.plan(),
        Err(TestError::Plan(Error::UnsupportedEdgeFlags { parent, child })) if parent == "app" && child == "dep"
    ));
    Ok(())
}

#[test]
fn context_verification_compares_whole_records() -> Result<(), TestError> {
    let fixture = Fixture::new()?;
    let mut changed = Fixture::new()?;
    let dep = changed.dep().clone();
    changed.units[1] = library(&dep, &[], &["other"]);
    assert!(matches!(
        verify_units(&fixture.graph()?, &changed.graph()?),
        Err(Error::ExternalContextsChanged {
            missing: 1,
            extra: 1
        })
    ));
    verify_units(&fixture.graph()?, &Fixture::new()?.graph()?)?;
    Ok(())
}
