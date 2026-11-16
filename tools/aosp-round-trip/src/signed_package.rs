// SPDX-License-Identifier: GPL-3.0-only
use crate::command_step::run_step;
use crate::error::{CommandStep, RoundTripError};
use crate::package_alignment::PackageAlignment;
use crate::signing_key::{KEY_ALIAS, SigningKey};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct BuildTools {
    directory: PathBuf,
}

impl BuildTools {
    #[must_use]
    pub fn in_sdk(sdk: &Path, version: &str) -> Self {
        Self {
            directory: sdk.join("build-tools").join(version),
        }
    }

    fn check_alignment(
        &self,
        alignment: PackageAlignment,
        package: &Path,
    ) -> Result<(), RoundTripError> {
        run_step(
            CommandStep::CheckAlignment,
            Command::new(self.directory.join("zipalign"))
                .arg("-c")
                .args(alignment.zipalign_arguments())
                .arg(package),
        )
    }

    fn apksigner(&self) -> Command {
        Command::new(self.directory.join("apksigner"))
    }
}

pub fn sign_package(
    build_tools: &BuildTools,
    alignment: PackageAlignment,
    unsigned: &Path,
    signed: &Path,
) -> Result<(), RoundTripError> {
    build_tools.check_alignment(alignment, unsigned)?;
    let key = SigningKey::open()?;
    let mut password_source = OsString::from("file:");
    password_source.push(&key.password);
    run_step(
        CommandStep::SignPackage,
        build_tools
            .apksigner()
            .arg("sign")
            .arg("--ks")
            .arg(&key.keystore)
            .args(["--ks-key-alias", KEY_ALIAS])
            .arg("--ks-pass")
            .arg(password_source)
            .args(["--v4-signing-enabled", "false"])
            .arg("--out")
            .arg(signed)
            .arg(unsigned),
    )?;
    run_step(
        CommandStep::VerifySignature,
        build_tools.apksigner().arg("verify").arg(signed),
    )?;
    build_tools.check_alignment(alignment, signed)
}
