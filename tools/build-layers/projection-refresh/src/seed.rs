// SPDX-License-Identifier: GPL-3.0-only
use crate::error::Error;
use crate::files::{create_directory, write_text};
use lavashell_cargo_model::contract::{ProjectionContract, StubLayout};
use lavashell_cargo_model::projection::{Layer, SeedHash, StubTarget};
use lavashell_command_output::nix::NixEvaluator;
use serde::{Deserialize, Serialize};
use std::path::Path;

const SEED_PLAN: &str = "nix/refresh-seed-plan.nix";

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeedWorkspace {
    pub manifest: String,
    pub lock: String,
    pub crates: Vec<SeedCrate>,
    pub selected_packages: Vec<String>,
    pub seed_hash: SeedHash,
}

#[derive(Clone, Debug, Deserialize)]
pub struct SeedCrate {
    pub member: String,
    pub manifest: String,
    pub targets: Vec<StubTarget>,
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
    pub projections: &'refresh Path,
    pub contract: &'refresh ProjectionContract,
}

impl Seeds<'_> {
    pub fn evaluate(&self, relative: &str, layer: Layer) -> Result<SeedWorkspace, Error> {
        Ok(self.nix.evaluate(
            &self.root.join(SEED_PLAN),
            &SeedArguments {
                root: self.root,
                projections: self.projections,
                relative,
                layer,
            },
        )?)
    }

    pub fn write(
        &self,
        relative: &str,
        layer: Layer,
        destination: &Path,
    ) -> Result<SeedWorkspace, Error> {
        let seed = self.evaluate(relative, layer)?;
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

fn write_stub_targets(
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
