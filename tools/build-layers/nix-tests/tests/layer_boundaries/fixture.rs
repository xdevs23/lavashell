// SPDX-License-Identifier: GPL-3.0-only
use crate::manifests::{Dependency, MemberManifest, WorkspaceManifest};
use lavashell_cargo_model::lock::CargoLock;
use lavashell_enum_values::enum_with_values;
use lavashell_nix_tests::evaluation::evaluate;
use lavashell_nix_tests::fixture_error::FixtureError;
use lavashell_nix_tests::repository::copy_repository;
use lavashell_nix_tests::toml_files::edit_toml;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use tempfile::TempDir;

const EXPRESSION: &str = "layer-derivations.nix";
const STALE_EXPRESSION: &str = "stale-layer-derivations.nix";

enum_with_values! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub enum LayerDerivation {
        Registry,
        Framework,
        Engine,
        ReferenceFramework,
        Reference,
        Native,
    }

    const ALL;
}

impl LayerDerivation {
    fn is_on_engine(self) -> bool {
        match self {
            Self::Engine | Self::ReferenceFramework | Self::Reference => true,
            Self::Registry | Self::Framework | Self::Native => false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Derivation {
    Layer(LayerDerivation),
    OfPackagedCrate(String),
}

impl Derivation {
    pub fn layers_on_engine() -> Vec<Self> {
        (LayerDerivation::ALL.into_iter())
            .filter(|layer| layer.is_on_engine())
            .map(Self::Layer)
            .collect()
    }
}

#[derive(Deserialize)]
pub struct CrateWithEngine {
    pub engine: Derivation,
    pub framework: Derivation,
    pub package: Derivation,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PackagesByRoute {
    pub registry_only: Vec<Derivation>,
    pub framework: Vec<Derivation>,
    pub framework_with_engine: Vec<CrateWithEngine>,
}

impl PackagesByRoute {
    fn engines(&self) -> impl Iterator<Item = &Derivation> {
        self.framework_with_engine
            .iter()
            .map(|with_engine| &with_engine.engine)
    }

    pub fn built_on_framework(&self) -> Vec<Derivation> {
        self.framework
            .iter()
            .chain(
                self.framework_with_engine
                    .iter()
                    .flat_map(|with_engine| [&with_engine.framework, &with_engine.package]),
            )
            .cloned()
            .collect()
    }

    pub fn framework_crate(&self, name: &str) -> Result<&Derivation, LayerTestError> {
        self.framework
            .iter()
            .find(|package| is_package_named(package, name))
            .ok_or_else(|| LayerTestError::MissingFrameworkCrate {
                name: name.to_owned(),
            })
    }

    pub fn crate_with_engine(&self, name: &str) -> Result<&CrateWithEngine, LayerTestError> {
        self.framework_with_engine
            .iter()
            .find(|with_engine| is_package_named(&with_engine.package, name))
            .ok_or_else(|| LayerTestError::MissingCrateWithEngine {
                name: name.to_owned(),
            })
    }
}

fn is_package_named(package: &Derivation, name: &str) -> bool {
    matches!(package, Derivation::OfPackagedCrate(packaged) if packaged == name)
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Variant {
    Plain,
    GuiTools,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
enum TreeKey {
    Directory,
    PackagesByRoute,
}

#[derive(Serialize)]
struct DerivationArguments<'arguments, Key> {
    root: &'arguments Path,
    variant: Variant,
    keys: &'arguments [Key],
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Tree {
    directory: PathBuf,
    packages_by_route: PackagesByRoute,
}

pub type Layers = BTreeMap<Derivation, String>;

#[derive(Debug, thiserror::Error)]
pub enum LayerTestError {
    #[error(transparent)]
    Fixture(#[from] FixtureError),
    #[error("the unedited repository cannot be evaluated: {0}")]
    Base(Arc<LayerTestError>),
    #[error("the reference directory {directory} is outside the repository")]
    ReferenceOutside { directory: PathBuf },
    #[error("{name} is not a packaged crate on the framework route")]
    MissingFrameworkCrate { name: String },
    #[error("{name} is not a packaged crate on the route with an engine layer")]
    MissingCrateWithEngine { name: String },
    #[error("{version} does not end in a patch number")]
    UnbumpableVersion { version: String },
    #[error("the workspace has no dependency {name} with a version")]
    MissingDependency { name: String },
    #[error("the lock contains {count} packages {name} {version}")]
    LockedPackageCount {
        name: String,
        version: String,
        count: usize,
    },
}

impl From<std::io::Error> for LayerTestError {
    fn from(error: std::io::Error) -> Self {
        Self::Fixture(FixtureError::from(error))
    }
}

pub struct Base {
    pub layers: Layers,
    pub reference: PathBuf,
    pub packages: PackagesByRoute,
}

impl Base {
    pub fn all(&self) -> Vec<Derivation> {
        self.all_except(&[])
    }

    pub fn all_except(&self, excluded: &[Derivation]) -> Vec<Derivation> {
        self.layers
            .keys()
            .filter(|layer| !excluded.contains(layer))
            .cloned()
            .collect()
    }

    pub fn engines(&self) -> Vec<Derivation> {
        std::iter::once(Derivation::Layer(LayerDerivation::Engine))
            .chain(self.packages.engines().cloned())
            .collect()
    }
}

pub struct Fixture(TempDir);

impl Fixture {
    pub fn new() -> Result<Self, FixtureError> {
        let directory = tempfile::tempdir()?;
        copy_repository(&directory.path().join("repository"))?;
        Ok(Self(directory))
    }

    pub fn root(&self) -> PathBuf {
        self.0.path().join("repository")
    }

    fn evaluate_keys<Value: DeserializeOwned>(
        &self,
        expression: &str,
        variant: Variant,
        keys: &[impl Serialize],
    ) -> Result<Value, FixtureError> {
        evaluate(
            expression,
            &DerivationArguments {
                root: &self.root(),
                variant,
                keys,
            },
        )
    }

    pub fn layers(&self, variant: Variant, keys: &[Derivation]) -> Result<Layers, FixtureError> {
        self.evaluate_keys(EXPRESSION, variant, keys)
    }

    pub fn all_layers(&self) -> Result<Layers, LayerTestError> {
        Ok(self.layers(Variant::Plain, &base()?.all())?)
    }

    fn tree(&self) -> Result<Tree, FixtureError> {
        self.evaluate_keys(
            EXPRESSION,
            Variant::Plain,
            &[TreeKey::Directory, TreeKey::PackagesByRoute],
        )
    }

    pub fn stale_layers(&self) -> Result<Vec<Derivation>, LayerTestError> {
        let mut stale: Vec<Derivation> =
            self.evaluate_keys(STALE_EXPRESSION, Variant::Plain, &base()?.all())?;
        stale.sort();
        Ok(stale)
    }

    pub fn edit_lock(
        &self,
        edit: impl FnOnce(&mut CargoLock) -> Result<(), LayerTestError>,
    ) -> Result<(), LayerTestError> {
        edit_toml(&self.root().join("Cargo.lock"), edit)
    }

    pub fn edit_workspace(
        &self,
        edit: impl FnOnce(&mut WorkspaceManifest) -> Result<(), LayerTestError>,
    ) -> Result<(), LayerTestError> {
        edit_toml(&self.root().join("Cargo.toml"), edit)
    }

    pub fn edit_member(
        &self,
        member: &Path,
        edit: impl FnOnce(&mut MemberManifest) -> Result<(), LayerTestError>,
    ) -> Result<(), LayerTestError> {
        edit_toml(&self.root().join(member).join("Cargo.toml"), edit)
    }

    pub fn bump_workspace_dependency(&self, name: &str) -> Result<(), LayerTestError> {
        let mut old = String::new();
        self.edit_workspace(|manifest| {
            let version = manifest
                .workspace
                .dependencies
                .get_mut(name)
                .and_then(Dependency::version_mut)
                .ok_or_else(|| LayerTestError::MissingDependency {
                    name: name.to_owned(),
                })?;
            version.trim_start_matches('=').clone_into(&mut old);
            *version = format!("={}", bumped(&old)?);
            Ok(())
        })?;
        self.edit_lock(|lock| bump_locked_package(lock, name, &old, &bumped(&old)?))
    }
}

fn evaluate_base() -> Result<Base, LayerTestError> {
    let fixture = Fixture::new()?;
    let tree = fixture.tree()?;
    let reference = tree
        .directory
        .strip_prefix(fixture.root())
        .map(Path::to_owned)
        .map_err(|_outside| LayerTestError::ReferenceOutside {
            directory: tree.directory.clone(),
        })?;
    let packages = tree.packages_by_route;
    let all: Vec<_> = (LayerDerivation::ALL.into_iter())
        .map(Derivation::Layer)
        .chain(packages.registry_only.iter().cloned())
        .chain(packages.engines().cloned())
        .chain(packages.built_on_framework())
        .collect();
    Ok(Base {
        layers: fixture.layers(Variant::Plain, &all)?,
        reference,
        packages,
    })
}

pub fn base() -> Result<&'static Base, LayerTestError> {
    static BASE: OnceLock<Result<Base, Arc<LayerTestError>>> = OnceLock::new();
    BASE.get_or_init(|| evaluate_base().map_err(Arc::new))
        .as_ref()
        .map_err(|error| LayerTestError::Base(Arc::clone(error)))
}

pub fn bumped(version: &str) -> Result<String, LayerTestError> {
    let unbumpable = || LayerTestError::UnbumpableVersion {
        version: version.to_owned(),
    };
    let (leading, patch) = version.rsplit_once('.').ok_or_else(unbumpable)?;
    let patch: u64 = patch.parse().map_err(|_not_a_number| unbumpable())?;
    Ok(format!("{leading}.{}", patch + 1))
}

pub fn bump_locked_package(
    lock: &mut CargoLock,
    name: &str,
    old: &str,
    new: &str,
) -> Result<(), LayerTestError> {
    let mut matching: Vec<_> = lock
        .package
        .iter_mut()
        .filter(|package| package.name == name && package.version == old)
        .collect();
    match matching.as_mut_slice() {
        [package] => {
            new.clone_into(&mut package.version);
            Ok(())
        }
        others => Err(LayerTestError::LockedPackageCount {
            name: name.to_owned(),
            version: old.to_owned(),
            count: others.len(),
        }),
    }
}
