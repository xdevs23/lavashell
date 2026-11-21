// SPDX-License-Identifier: GPL-3.0-only
use crate::error::Error;
use crate::files::{create_directory, parse_toml, write_text};
use lavashell_cargo_model::contract::{ProjectionContract, StubLayout};
use lavashell_cargo_model::manifest::{BuildScript, PackageManifest};
use lavashell_cargo_model::projection::{Layer, StubTarget};
use lavashell_command_output::nix::NixEvaluator;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const SEED_PLAN: &str = "nix/refresh-seed-plan.nix";

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeedWorkspace {
    pub manifest: String,
    pub lock: String,
    pub crates: Vec<SeedCrate>,
    pub seed_hash: String,
    #[serde(default)]
    pub original_contracts: Vec<OriginalContract>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct SeedCrate {
    pub member: String,
    pub manifest: String,
    pub targets: Vec<StubTarget>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct OriginalContract {
    pub member: String,
    pub links: Option<String>,
    pub build: BuildScript,
}

#[derive(Clone, Debug, Deserialize)]
pub struct SeedManifest {
    pub workspace: SeedWorkspaceSection,
    pub profile: Option<toml::Table>,
    pub patch: Option<toml::Table>,
    pub replace: Option<toml::Table>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct SeedWorkspaceSection {
    pub resolver: Option<String>,
}

#[derive(Serialize)]
struct SeedArguments<'arguments> {
    root: &'arguments Path,
    projections: &'arguments Path,
    relative: &'arguments str,
    layer: Layer,
}

pub struct Seeds<'refresh> {
    pub nix: &'refresh NixEvaluator,
    pub root: &'refresh Path,
    pub contract: &'refresh ProjectionContract,
}

impl Seeds<'_> {
    pub fn write(
        &self,
        projections: &Path,
        relative: &str,
        layer: Layer,
        destination: &Path,
    ) -> Result<SeedWorkspace, Error> {
        let seed: SeedWorkspace = self.nix.evaluate(
            &self.root.join(SEED_PLAN),
            &SeedArguments {
                root: self.root,
                projections,
                relative,
                layer,
            },
        )?;
        write_text(&destination.join("Cargo.toml"), &seed.manifest)?;
        write_text(&destination.join("Cargo.lock"), &seed.lock)?;
        for member in &seed.crates {
            let directory = destination.join(&member.member);
            write_text(&directory.join("Cargo.toml"), &member.manifest)?;
            write_stub_targets(&directory, &member.targets, &self.contract.stub)?;
        }
        Ok(seed)
    }
}

impl SeedWorkspace {
    pub fn workspace_manifest(&self) -> Result<SeedManifest, Error> {
        parse_toml(&PathBuf::from("Cargo.toml"), &self.manifest)
    }

    pub fn companions(&self, contract: &ProjectionContract) -> Result<Vec<String>, Error> {
        let names = self
            .crates
            .iter()
            .map(|member| {
                parse_toml::<PackageManifest>(
                    &Path::new(&member.member).join("Cargo.toml"),
                    &member.manifest,
                )
                .map(|manifest| manifest.package.name)
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(contract
            .companions
            .iter()
            .filter(|companion| names.contains(companion))
            .cloned()
            .collect())
    }
}

pub fn write_stub_targets(
    member: &Path,
    targets: &[StubTarget],
    stub: &StubLayout,
) -> Result<(), Error> {
    create_directory(member)?;
    targets.iter().try_for_each(|target| {
        let source = if target.main {
            &stub.main_source
        } else {
            &stub.library_source
        };
        write_text(&member.join(&target.path), &format!("{source}\n"))
    })
}
