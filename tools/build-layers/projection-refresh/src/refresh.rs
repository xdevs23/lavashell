// SPDX-License-Identifier: GPL-3.0-only
use crate::activations::missing_activations;
use crate::canonical::{Seed, canonical_plan, materialize, verify_units};
use crate::cargo::{Cargo, LockMode, ResolvedGraph};
use crate::concurrency::run_concurrently;
use crate::error::Error;
use crate::files::{read_toml, write_json, write_toml};
use crate::projection_start::{ProjectionStart, start_native_projection, start_package_projection};
use crate::seed::{SeedWorkspace, Seeds};
use lavashell_cargo_model::contract::ProjectionContract;
use lavashell_cargo_model::lock::{CargoLock, LockedPackage};
use lavashell_cargo_model::manifest::PackageManifest;
use lavashell_cargo_model::projection::{Layer, LayerPlan, LockedPlan, ProjectionFile, Selection};
use lavashell_cargo_model::unit_graph::UnitGraph;
use lavashell_plan_check::{external_records, verify};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const CATALOG: &str = "nix/refresh-catalog.nix";

pub fn package_name(directory: &Path) -> Result<String, Error> {
    read_toml::<PackageManifest>(&directory.join("Cargo.toml"))
        .map(|manifest| manifest.package.name)
}

#[derive(Clone, Debug)]
pub struct PackageEntry {
    pub directory: String,
    pub package: String,
}

#[derive(Clone, Debug)]
struct LayeredPackage {
    entry: PackageEntry,
    layer: Layer,
}

#[derive(Serialize)]
struct CatalogArguments<'arguments> {
    root: &'arguments Path,
    projections: &'arguments Path,
    directories: Vec<&'arguments str>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LayerCatalog {
    needs_engine: BTreeMap<String, bool>,
    native_names: Vec<String>,
}

pub struct Toolkit<'refresh> {
    pub cargo: &'refresh Cargo,
    pub seeds: Seeds<'refresh>,
    pub contract: &'refresh ProjectionContract,
    pub root: &'refresh Path,
}

impl Toolkit<'_> {
    fn graph(
        &self,
        directory: &Path,
        package: &str,
        companions: &[String],
        mode: LockMode,
    ) -> Result<ResolvedGraph, Error> {
        let packages: Vec<String> = std::iter::once(package.to_owned())
            .chain(companions.iter().cloned())
            .collect();
        self.cargo.unit_graph(directory, &packages, mode)
    }

    fn seed_graph(
        &self,
        relative: &str,
        layer: Layer,
        destination: &Path,
    ) -> Result<ResolvedGraph, Error> {
        let seed = self.seeds.write(relative, layer, destination)?;
        let package = package_name(&self.root.join(relative))?;
        let companions = seed.companions(self.contract)?;
        self.graph(destination, &package, &companions, LockMode::Resolve)
    }

    fn dependency_plan(
        &self,
        relative: &str,
        layer: Layer,
        destination: &Path,
    ) -> Result<LayerPlan, Error> {
        let seed_path = destination.join("seed");
        let seed = self.seeds.write(relative, layer, &seed_path)?;
        let package = package_name(&self.root.join(relative))?;
        let companions = seed.companions(self.contract)?;
        let expected = self.graph(&seed_path, &package, &companions, LockMode::Frozen)?;
        let metadata = self.cargo.metadata(&seed_path)?;
        let selection = Selection {
            target: self.cargo.target.clone(),
            profile: self.contract.profile.clone(),
            packages: std::iter::once(package.clone())
                .chain(companions.iter().cloned())
                .collect(),
            features: Vec::new(),
        };
        let manifest = seed.workspace_manifest()?;
        let plan = canonical_plan(
            &Seed {
                manifest: &manifest,
                original_contracts: &seed.original_contracts,
            },
            &expected.graph,
            &metadata,
            selection,
            self.contract,
        )?;
        let canonical_path = destination.join("canonical");
        materialize(&plan, &canonical_path, &expected.lock, self.contract)?;
        let resolved = self.graph(&canonical_path, &package, &companions, LockMode::Resolve)?;
        verify_units(&expected.graph, &resolved.graph)?;
        write_toml(&canonical_path.join("Cargo.lock"), &resolved.lock)?;
        let frozen = self.graph(&canonical_path, &package, &companions, LockMode::Frozen)?;
        verify_units(&expected.graph, &frozen.graph)?;
        Ok(LayerPlan {
            seed_hash: seed.seed_hash,
            plan: LockedPlan {
                plan,
                lock: resolved.lock,
                external_units: external_records(&frozen.graph)?,
            },
        })
    }

    fn catalog(
        &self,
        entries: &[PackageEntry],
    ) -> Result<(Vec<LayeredPackage>, Vec<String>), Error> {
        let catalog: LayerCatalog = self.seeds.nix.evaluate(
            &self.root.join(CATALOG),
            &CatalogArguments {
                root: self.root,
                projections: self.seeds.projections,
                directories: entries
                    .iter()
                    .map(|entry| entry.directory.as_str())
                    .collect(),
            },
        )?;
        let layered = entries
            .iter()
            .map(|entry| {
                let needs_engine = catalog
                    .needs_engine
                    .get(&entry.directory)
                    .copied()
                    .ok_or_else(|| Error::CatalogIncomplete {
                        directory: entry.directory.clone(),
                    })?;
                Ok(LayeredPackage {
                    entry: entry.clone(),
                    layer: if needs_engine {
                        Layer::Engine
                    } else {
                        Layer::Registry
                    },
                })
            })
            .collect::<Result<_, Error>>()?;
        Ok((layered, catalog.native_names))
    }
}

fn engine_seed_lock(
    original: &CargoLock,
    baseline: &CargoLock,
    native_names: &[String],
) -> CargoLock {
    let external = baseline.external_packages().cloned();
    let local = original.local_packages().map(|package| LockedPackage {
        dependencies: package
            .dependencies
            .iter()
            .filter(|reference| !native_names.contains(&reference.name))
            .cloned()
            .collect(),
        ..package.clone()
    });
    CargoLock {
        version: baseline.version,
        package: external.chain(local).collect(),
    }
}

struct LayerRefresh<'refresh> {
    toolkit: &'refresh Toolkit<'refresh>,
    original: &'refresh CargoLock,
    native_names: &'refresh [String],
    output: &'refresh Path,
    scratch: &'refresh Path,
}

impl LayerRefresh<'_> {
    fn projection_path(&self, entry: &PackageEntry) -> PathBuf {
        self.output.join(format!("{}.json", entry.package))
    }

    fn write(&self, entry: &PackageEntry, projection: &ProjectionFile) -> Result<(), Error> {
        write_json(&self.projection_path(entry), projection)
    }

    fn normal_graph(&self, entry: &PackageEntry) -> Result<ResolvedGraph, Error> {
        let destination = self.scratch.join(&entry.package).join("normal");
        self.toolkit
            .seed_graph(&entry.directory, Layer::Registry, &destination)
    }

    fn store_plan(
        &self,
        package: &LayeredPackage,
        projection: &mut ProjectionFile,
    ) -> Result<(), Error> {
        let layer_name = match package.layer {
            Layer::Engine => "engine-layer",
            Layer::Registry => "registry-layer",
        };
        let destination = self.scratch.join(&package.entry.package).join(layer_name);
        let plan =
            self.toolkit
                .dependency_plan(&package.entry.directory, package.layer, &destination)?;
        let _plan_of_previous_refresh: Option<LayerPlan> = projection
            .dependency_plans
            .entry(self.toolkit.cargo.target.clone())
            .or_default()
            .insert(package.layer, plan);
        self.write(&package.entry, projection)
    }

    fn refresh_registry_layer(
        &self,
        package: &LayeredPackage,
        mut projection: ProjectionFile,
    ) -> Result<Option<usize>, Error> {
        projection.normal_lock = self.normal_graph(&package.entry)?.lock;
        self.write(&package.entry, &projection)?;
        self.store_plan(package, &mut projection)?;
        Ok(None)
    }

    fn refresh_engine_layer(
        &self,
        package: &LayeredPackage,
        mut projection: ProjectionFile,
    ) -> Result<Option<usize>, Error> {
        let entry = &package.entry;
        let mut features = BTreeMap::new();
        projection.features = Some(features.clone());
        let baseline = projection.engine_lock.as_ref().unwrap_or(self.original);
        projection.engine_lock = Some(engine_seed_lock(self.original, baseline, self.native_names));
        self.write(entry, &projection)?;
        let normal = self.normal_graph(entry)?;
        projection.normal_lock = normal.lock;
        self.write(entry, &projection)?;
        for attempt in 0.. {
            let destination = self
                .scratch
                .join(&entry.package)
                .join(format!("engine-{attempt}"));
            let _seed_read_back_by_cargo: SeedWorkspace =
                self.toolkit
                    .seeds
                    .write(&entry.directory, Layer::Engine, &destination)?;
            let engine =
                self.toolkit
                    .graph(&destination, &entry.package, &[], LockMode::Resolve)?;
            projection.engine_lock = Some(engine.lock);
            let missing = missing_activations(&engine.graph, &normal.graph)?;
            if missing.is_empty() {
                return self.finish_engine_layer(package, projection, &engine.graph, &normal.graph);
            }
            let before = features.clone();
            for (kind, dependencies) in missing {
                features
                    .entry(kind)
                    .or_insert_with(BTreeMap::new)
                    .extend(dependencies);
            }
            if features == before {
                return Err(Error::SharedFeaturesUnchanged {
                    package: entry.package.clone(),
                });
            }
            projection.features = Some(features.clone());
            self.write(entry, &projection)?;
        }
        Err(Error::SharedFeaturesUnchanged {
            package: entry.package.clone(),
        })
    }

    fn finish_engine_layer(
        &self,
        package: &LayeredPackage,
        mut projection: ProjectionFile,
        engine: &UnitGraph,
        normal: &UnitGraph,
    ) -> Result<Option<usize>, Error> {
        let records = external_records(engine)?;
        let _units_counted_by_records: lavashell_plan_check::CompatiblePlan =
            verify(&records, normal, &self.toolkit.contract.engine_root)?;
        let count = records.len();
        projection.external_units = Some(records);
        self.write(&package.entry, &projection)?;
        self.store_plan(package, &mut projection)?;
        Ok(Some(count))
    }

    fn refresh(&self, package: &LayeredPackage) -> Result<RefreshedPackage, Error> {
        let ProjectionStart {
            projection,
            unresolved_stored_crate,
        } = start_package_projection(
            &self.projection_path(&package.entry),
            package.layer,
            &self.toolkit.cargo.target,
            self.original,
        )?;
        match package.layer {
            Layer::Engine => self.refresh_engine_layer(package, projection),
            Layer::Registry => self.refresh_registry_layer(package, projection),
        }
        .map(|external_units| RefreshedPackage {
            package: package.entry.package.clone(),
            external_units,
            unresolved_stored_crate,
        })
        .map_err(|source| Error::Refresh {
            package: package.entry.package.clone(),
            source: Box::new(source),
        })
    }
}

pub struct RefreshedPackage {
    pub package: String,
    pub external_units: Option<usize>,
    pub unresolved_stored_crate: Option<LockedPackage>,
}

pub fn refresh_projections(
    toolkit: &Toolkit,
    entries: &[PackageEntry],
) -> Result<Vec<RefreshedPackage>, Error> {
    let contract = toolkit.contract;
    let original: CargoLock = read_toml(&toolkit.root.join("Cargo.lock"))?;
    let output = toolkit.seeds.projections;
    let native_path = output.join("native.json");
    let native_start = start_native_projection(&native_path, &original)?;
    let scratch = tempfile::tempdir().map_err(Error::TemporaryDirectory)?;
    let native_graph = toolkit.seed_graph(
        &contract.native_directory,
        Layer::Registry,
        &scratch.path().join("native"),
    )?;
    let mut native = native_start
        .projection
        .prepared(native_graph.lock, &toolkit.cargo.target);
    write_json(&native_path, &native)?;
    let (packages, native_names) = toolkit.catalog(entries)?;
    let native_plan = toolkit.dependency_plan(
        &contract.native_directory,
        Layer::Registry,
        &scratch.path().join("native-layer"),
    )?;
    let _plan_of_previous_refresh: Option<LayerPlan> = native
        .dependency_plans
        .entry(toolkit.cargo.target.clone())
        .or_default()
        .insert(Layer::Registry, native_plan);
    write_json(&native_path, &native)?;
    let refresh = LayerRefresh {
        toolkit,
        original: &original,
        native_names: &native_names,
        output,
        scratch: scratch.path(),
    };
    let native_refresh = RefreshedPackage {
        package: package_name(&toolkit.root.join(&contract.native_directory))?,
        external_units: None,
        unresolved_stored_crate: native_start.unresolved_stored_crate,
    };
    let refreshed = run_concurrently(
        packages
            .iter()
            .map(|package| || refresh.refresh(package))
            .collect(),
    )?;
    Ok(std::iter::once(native_refresh).chain(refreshed).collect())
}
