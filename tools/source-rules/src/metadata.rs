// SPDX-License-Identifier: GPL-3.0-only
// https://doc.rust-lang.org/cargo/commands/cargo-metadata.html#json-format
use crate::error::Error;
use crate::repository_files::{FileKind, RepositoryFile};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub const MANIFEST: &str = "Cargo.toml";

#[derive(Deserialize)]
struct AnyManifest {
    workspace: Option<serde::de::IgnoredAny>,
}

#[derive(Deserialize)]
pub struct Workspace {
    pub workspace_root: PathBuf,
    pub packages: Vec<Package>,
}

#[derive(Deserialize)]
pub struct Package {
    manifest_path: PathBuf,
    pub targets: Vec<Target>,
    dependencies: Vec<Dependency>,
}

#[derive(Deserialize)]
pub struct Target {
    name: String,
    kind: Vec<TargetKind>,
    pub src_path: PathBuf,
}

#[derive(Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
enum TargetKind {
    Lib,
    Rlib,
    Dylib,
    ProcMacro,
    #[serde(other)]
    NotLinkable,
}

#[derive(Deserialize)]
struct Dependency {
    rename: Option<String>,
    path: Option<PathBuf>,
}

impl Package {
    fn library(&self) -> Option<&Target> {
        (self.targets.iter())
            .find(|target| (target.kind.iter()).any(|kind| *kind != TargetKind::NotLinkable))
    }

    fn directory(&self) -> Option<&Path> {
        self.manifest_path.parent()
    }
}

// https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html#renaming-dependencies-in-cargotoml
fn crate_name_of_rename(rename: &str) -> String {
    rename.replace('-', "_")
}

impl Workspace {
    pub fn crate_roots(&self) -> Vec<PathBuf> {
        self.packages
            .iter()
            .flat_map(|package| &package.targets)
            .map(|target| target.src_path.clone())
            .collect()
    }

    pub fn read(directory: &Path) -> Result<Self, Error> {
        let output = Command::new("cargo")
            .current_dir(directory)
            .args([
                "metadata",
                "--no-deps",
                "--offline",
                "--locked",
                "--format-version",
                "1",
            ])
            .stderr(Stdio::inherit())
            .output()
            .map_err(|source| Error::StartMetadata {
                directory: directory.to_owned(),
                source,
            })?;
        if !output.status.success() {
            return Err(Error::Metadata {
                directory: directory.to_owned(),
                status: output.status,
            });
        }
        serde_json::from_slice(&output.stdout).map_err(Error::ReadMetadata)
    }
}

pub fn nested_workspace_directories(
    files: &[RepositoryFile],
    root: &Path,
) -> Result<Vec<PathBuf>, Error> {
    let root_manifest = root.join(MANIFEST);
    files
        .iter()
        .filter_map(|file| match &file.kind {
            Some(FileKind::Source { text, .. })
                if file.path.ends_with(MANIFEST) && file.path != root_manifest =>
            {
                Some((&file.path, text))
            }
            _ => None,
        })
        .filter_map(|(path, text)| match toml::from_str::<AnyManifest>(text) {
            Ok(AnyManifest { workspace: None }) => None,
            Ok(AnyManifest { workspace: Some(_) }) => Some(
                path.parent()
                    .map(Path::to_path_buf)
                    .ok_or_else(|| Error::NoParentDirectory { path: path.clone() }),
            ),
            Err(source) => Some(Err(Error::ReadManifest {
                path: path.clone(),
                source,
            })),
        })
        .collect()
}

pub struct Workspaces<'root> {
    root: &'root Workspace,
    nested: Vec<Workspace>,
}

impl<'root> Workspaces<'root> {
    pub fn read(root: &'root Workspace, nested_directories: &[PathBuf]) -> Result<Self, Error> {
        let nested = (nested_directories.iter())
            .map(|directory| Workspace::read(directory))
            .collect::<Result<_, Error>>()?;
        Ok(Self { root, nested })
    }

    pub fn crate_roots(&self) -> Vec<PathBuf> {
        self.every_workspace()
            .flat_map(Workspace::crate_roots)
            .collect()
    }

    pub fn extern_preludes(&self) -> Result<ExternPreludes, Error> {
        let packages: Vec<&Package> = (self.every_workspace())
            .flat_map(|workspace| &workspace.packages)
            .collect();
        let libraries_by_directory: BTreeMap<&Path, &Target> = (packages.iter())
            .filter_map(|package| Some((package.directory()?, package.library()?)))
            .collect();
        let mut preludes = BTreeMap::new();
        for package in packages {
            let dependencies = (package.dependencies.iter()).filter_map(|dependency| {
                let library = libraries_by_directory.get(dependency.path.as_deref()?)?;
                let name = (dependency.rename.as_deref())
                    .map_or_else(|| library.name.clone(), crate_name_of_rename);
                Some((name, library.src_path.clone()))
            });
            let own_library =
                (package.library()).map(|library| (library.name.clone(), library.src_path.clone()));
            let prelude = extern_prelude(
                &package.manifest_path,
                own_library.into_iter().chain(dependencies),
            )?;
            preludes.extend(
                (package.targets.iter()).map(|target| (target.src_path.clone(), prelude.clone())),
            );
        }
        Ok(ExternPreludes(preludes))
    }

    fn every_workspace(&self) -> impl Iterator<Item = &Workspace> {
        std::iter::once(self.root).chain(&self.nested)
    }
}

// https://doc.rust-lang.org/reference/names/preludes.html#extern-prelude
pub struct ExternPreludes(BTreeMap<PathBuf, BTreeMap<String, PathBuf>>);

impl ExternPreludes {
    pub fn library_root(&self, crate_root: &Path, crate_name: &str) -> Option<&Path> {
        (self.0.get(crate_root)?.get(crate_name)).map(PathBuf::as_path)
    }
}

impl FromIterator<(PathBuf, BTreeMap<String, PathBuf>)> for ExternPreludes {
    fn from_iter<Preludes: IntoIterator<Item = (PathBuf, BTreeMap<String, PathBuf>)>>(
        preludes: Preludes,
    ) -> Self {
        Self(preludes.into_iter().collect())
    }
}

fn extern_prelude(
    manifest: &Path,
    crates: impl IntoIterator<Item = (String, PathBuf)>,
) -> Result<BTreeMap<String, PathBuf>, Error> {
    crates.into_iter().try_fold(
        BTreeMap::new(),
        |mut prelude: BTreeMap<String, PathBuf>, (name, library_root)| match prelude.get(&name) {
            Some(first) if *first != library_root => Err(Error::AmbiguousCrateName {
                manifest: manifest.to_owned(),
                name,
                first: first.clone(),
                second: library_root,
            }),
            _ => {
                prelude.extend([(name, library_root)]);
                Ok(prelude)
            }
        },
    )
}
