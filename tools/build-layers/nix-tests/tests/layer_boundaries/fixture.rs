// SPDX-License-Identifier: GPL-3.0-only
use crate::manifests::{Dependency, MemberManifest, WorkspaceManifest};
use lavashell_cargo_model::lock::CargoLock;
use lavashell_command_output::json::CommandError;
use lavashell_command_output::nix::NixError;
use lavashell_nix_tests::evaluation::evaluate;
use lavashell_nix_tests::fixture_error::FixtureError;
use lavashell_nix_tests::repository::copy_repository;
use lavashell_nix_tests::toml_files::edit_toml;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use tempfile::TempDir;

const EXPRESSION: &str = "layer-derivations.nix";

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Derivation {
    Registry,
    Framework,
    Engine,
    ReferenceFramework,
    Reference,
    Native,
    WaylandInput,
    AospRoundTrip,
    SourceRules,
}

impl Derivation {
    pub const ALL: [Self; 9] = [
        Self::Registry,
        Self::Framework,
        Self::Engine,
        Self::ReferenceFramework,
        Self::Reference,
        Self::Native,
        Self::WaylandInput,
        Self::AospRoundTrip,
        Self::SourceRules,
    ];
    pub const ON_ENGINE: [Self; 3] = [Self::Engine, Self::ReferenceFramework, Self::Reference];
    pub const OFF_ENGINE: [Self; 3] = [Self::Registry, Self::Framework, Self::Native];
    pub const OFF_FRAMEWORK: [Self; 3] =
        [Self::WaylandInput, Self::AospRoundTrip, Self::SourceRules];

    pub fn above_engine() -> Vec<Self> {
        Self::ALL
            .into_iter()
            .filter(|layer| *layer != Self::Engine)
            .collect()
    }
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Variant {
    Plain,
    GuiTools,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
enum DirectoryKey {
    Directory,
}

#[derive(Serialize)]
struct DerivationArguments<'arguments, Key> {
    root: &'arguments Path,
    variant: Variant,
    keys: &'arguments [Key],
}

#[derive(Deserialize)]
struct ReferenceDirectory {
    directory: PathBuf,
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

    pub fn layers(&self, variant: Variant, keys: &[Derivation]) -> Result<Layers, FixtureError> {
        evaluate(
            EXPRESSION,
            &DerivationArguments {
                root: &self.root(),
                variant,
                keys,
            },
        )
    }

    pub fn all_layers(&self) -> Result<Layers, FixtureError> {
        self.layers(Variant::Plain, &Derivation::ALL)
    }

    fn reference(&self) -> Result<PathBuf, LayerTestError> {
        let reference: ReferenceDirectory = evaluate(
            EXPRESSION,
            &DerivationArguments {
                root: &self.root(),
                variant: Variant::Plain,
                keys: &[DirectoryKey::Directory],
            },
        )?;
        reference
            .directory
            .strip_prefix(self.root())
            .map(Path::to_owned)
            .map_err(|_outside| LayerTestError::ReferenceOutside {
                directory: reference.directory.clone(),
            })
    }

    pub fn stale_layers(&self) -> Result<Vec<Derivation>, FixtureError> {
        let mut stale = Vec::new();
        for layer in Derivation::ALL {
            match self.layers(Variant::Plain, &[layer]) {
                Ok(_) => {}
                Err(FixtureError::Nix(NixError::Command(CommandError::Failed { .. }))) => {
                    stale.push(layer);
                }
                Err(error) => return Err(error),
            }
        }
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
    Ok(Base {
        layers: fixture.all_layers()?,
        reference: fixture.reference()?,
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
