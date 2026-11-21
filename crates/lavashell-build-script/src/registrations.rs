// SPDX-License-Identifier: GPL-3.0-only
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::{fmt, fs, io, iter};

use quote::{format_ident, quote};
use serde::Deserialize;
use serde::de::{DeserializeOwned, IgnoredAny};

use crate::{BuildScriptError, manifest_directory, output_directory, rerun_if_changed, write};

#[derive(Debug, thiserror::Error)]
pub enum RegistrationError {
    #[error(transparent)]
    Script(#[from] BuildScriptError),
    #[error("cannot read {}; make the file readable", .manifest.display())]
    ReadManifest {
        manifest: PathBuf,
        source: io::Error,
    },
    #[error("cannot parse {}; fix the TOML error that follows", .manifest.display())]
    ParseManifest {
        manifest: PathBuf,
        source: toml::de::Error,
    },
    #[error(
        "no Cargo.toml above {} has a [workspace] table; build the crate from inside its workspace",
        .0.display()
    )]
    WorkspaceNotFound(PathBuf),
    #[error(
        "{} takes {dependency} from the workspace, but [workspace.dependencies] in {} has no \
         {dependency}; add {dependency} there",
        .dependent_manifest.display(),
        .workspace_manifest.display()
    )]
    WorkspaceEntryMissing {
        dependent_manifest: PathBuf,
        dependency: String,
        workspace_manifest: PathBuf,
    },
    #[error("{}", MissingRegistrations(.missing))]
    RegistrationMissing { missing: Vec<MissingRegistration> },
    #[error(
        "{child} registers with {parent}, but {} does not list {parent}, so nothing runs what \
         {child} registers; add {parent} to the dependencies in that manifest",
        .manifest.display()
    )]
    ParentNotListed {
        child: String,
        parent: String,
        manifest: PathBuf,
    },
}

#[derive(Debug)]
pub struct MissingRegistration {
    dependent: String,
    extended: String,
    register_file: PathBuf,
}

struct MissingRegistrations<'a>(&'a [MissingRegistration]);

#[derive(Deserialize)]
struct DependencyTable {
    #[serde(default)]
    dependencies: BTreeMap<String, DependencyRequest>,
}

#[derive(Deserialize)]
struct Manifest {
    #[serde(flatten)]
    own: DependencyTable,
    #[serde(default)]
    target: BTreeMap<String, DependencyTable>,
}

#[derive(Deserialize)]
struct WorkspaceManifest {
    workspace: Option<DependencyTable>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum DependencyRequest {
    Detailed {
        path: Option<PathBuf>,
        #[serde(default)]
        workspace: bool,
    },
    Version(IgnoredAny),
}

struct Workspace {
    directory: PathBuf,
    dependencies: BTreeMap<String, DependencyRequest>,
}

struct LocalCrate {
    name: String,
    directory: PathBuf,
}

#[derive(Default)]
struct Inputs(BTreeSet<PathBuf>);

struct Registrations {
    registering_crates: Vec<String>,
    inputs: Inputs,
}

pub fn generate_registrations() -> Result<(), RegistrationError> {
    let registrations = Registrations::find(&manifest_directory()?)?;
    registrations
        .inputs
        .0
        .iter()
        .try_for_each(|input| rerun_if_changed(input))?;
    Ok(write(
        output_directory()?.join("registrations.rs"),
        registrations.register_all(),
    )?)
}

impl Registrations {
    fn find(crate_directory: &Path) -> Result<Self, RegistrationError> {
        let mut inputs = Inputs::default();
        let workspace = Workspace::find_above(crate_directory, &mut inputs)?;
        let listed = workspace.local_dependencies(crate_directory, &mut inputs)?;
        let listed_names: BTreeSet<&str> =
            listed.iter().map(|listed| listed.name.as_str()).collect();
        let mut registering_crates = Vec::new();
        let mut missing = Vec::new();
        for dependency in &listed {
            let registering_parents: Vec<LocalCrate> = (workspace
                .local_dependencies(&dependency.directory, &mut inputs)?
                .into_iter())
            .filter(LocalCrate::registers_itself)
            .collect();
            if dependency.registers_itself() {
                if let Some(parent) = (registering_parents.iter())
                    .find(|parent| !listed_names.contains(parent.name.as_str()))
                {
                    return Err(RegistrationError::ParentNotListed {
                        child: dependency.name.clone(),
                        parent: parent.name.clone(),
                        manifest: manifest_of(crate_directory),
                    });
                }
                registering_crates.push(dependency.name.clone());
            } else if let Some(extended) = registering_parents.first() {
                missing.push(MissingRegistration {
                    dependent: dependency.name.clone(),
                    extended: extended.name.clone(),
                    register_file: dependency.register_file(),
                });
            }
        }
        if !missing.is_empty() {
            return Err(RegistrationError::RegistrationMissing { missing });
        }
        Ok(Self {
            registering_crates,
            inputs,
        })
    }

    fn register_all(&self) -> String {
        let crates = (self.registering_crates.iter())
            .map(|name| format_ident!("{}", name.replace('-', "_")));
        quote! {
            fn register_all(store: &mut lavashell_store::Store) {
                #(#crates::register::register(store);)*
            }
        }
        .to_string()
    }
}

impl Workspace {
    fn find_above(crate_directory: &Path, inputs: &mut Inputs) -> Result<Self, RegistrationError> {
        for directory in crate_directory.ancestors().skip(1) {
            let manifest = manifest_of(directory);
            if !manifest.is_file() {
                continue;
            }
            let candidate: WorkspaceManifest = inputs.read_manifest(&manifest)?;
            if let Some(workspace) = candidate.workspace {
                return Ok(Self {
                    directory: directory.to_owned(),
                    dependencies: workspace.dependencies,
                });
            }
        }
        Err(RegistrationError::WorkspaceNotFound(
            crate_directory.to_owned(),
        ))
    }

    fn local_dependencies(
        &self,
        dependent_directory: &Path,
        inputs: &mut Inputs,
    ) -> Result<Vec<LocalCrate>, RegistrationError> {
        let manifest: Manifest = inputs.read_manifest(&manifest_of(dependent_directory))?;
        let mut local = Vec::new();
        for (name, request) in manifest.dependencies() {
            if let Some(directory) = self.locate(dependent_directory, name, request)? {
                inputs.0.extend([directory.join("src")]);
                local.push(LocalCrate {
                    name: name.to_owned(),
                    directory,
                });
            }
        }
        Ok(local)
    }

    fn locate(
        &self,
        dependent_directory: &Path,
        name: &str,
        request: &DependencyRequest,
    ) -> Result<Option<PathBuf>, RegistrationError> {
        match request {
            DependencyRequest::Detailed {
                workspace: true, ..
            } => {
                let entry = self.dependencies.get(name).ok_or_else(|| {
                    RegistrationError::WorkspaceEntryMissing {
                        dependent_manifest: manifest_of(dependent_directory),
                        dependency: name.to_owned(),
                        workspace_manifest: manifest_of(&self.directory),
                    }
                })?;
                Ok(entry.path().map(|path| self.directory.join(path)))
            }
            _ => Ok(request.path().map(|path| dependent_directory.join(path))),
        }
    }
}

impl Manifest {
    fn dependencies(&self) -> BTreeMap<&str, &DependencyRequest> {
        (iter::once(&self.own).chain(self.target.values()))
            .flat_map(|table| table.dependencies.iter())
            .map(|(name, request)| (name.as_str(), request))
            .collect()
    }
}

impl DependencyRequest {
    fn path(&self) -> Option<&Path> {
        match self {
            Self::Detailed { path, .. } => path.as_deref(),
            Self::Version(_) => None,
        }
    }
}

impl LocalCrate {
    fn register_file(&self) -> PathBuf {
        self.directory.join("src").join("register.rs")
    }

    fn registers_itself(&self) -> bool {
        self.register_file().is_file()
    }
}

impl Inputs {
    fn read_manifest<T: DeserializeOwned>(
        &mut self,
        manifest: &Path,
    ) -> Result<T, RegistrationError> {
        self.0.extend([manifest.to_owned()]);
        let text =
            fs::read_to_string(manifest).map_err(|source| RegistrationError::ReadManifest {
                manifest: manifest.to_owned(),
                source,
            })?;
        toml::from_str(&text).map_err(|source| RegistrationError::ParseManifest {
            manifest: manifest.to_owned(),
            source,
        })
    }
}

fn manifest_of(crate_directory: &Path) -> PathBuf {
    crate_directory.join("Cargo.toml")
}

impl fmt::Display for MissingRegistration {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{dependent} uses {extended}, which registers itself, so {dependent} has to register \
             too; add {} with `pub fn register(store: &mut Store)`",
            self.register_file.display(),
            dependent = self.dependent,
            extended = self.extended,
        )
    }
}

impl fmt::Display for MissingRegistrations<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, missing) in self.0.iter().enumerate() {
            if index > 0 {
                formatter.write_str("; ")?;
            }
            write!(formatter, "{missing}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
