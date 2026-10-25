// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]

use std::path::PathBuf;
use std::process::ExitCode;
use std::{fs, io};

use lavashell_build_script::{BuildScriptError, PackageVariable, output_directory, run, write};
use quote::quote;
use thiserror::Error;

const HYPRLAND_PROTOCOLS: PackageVariable = PackageVariable {
    name: "LAVASHELL_HYPRLAND_PROTOCOLS",
    package: "the hyprland-protocols package from nixpkgs",
};

#[derive(Debug, Error)]
enum BuildError {
    #[error(transparent)]
    Script(#[from] BuildScriptError),
    #[error(
        "cannot find the focus-grab protocol at {}. Set {} to {}.",
        .file.display(), HYPRLAND_PROTOCOLS.name, HYPRLAND_PROTOCOLS.package
    )]
    FindProtocol { file: PathBuf, source: io::Error },
    #[error(
        "the path {} is not UTF-8, which wayland-scanner needs. Move the package to a path in UTF-8.",
        .0.display()
    )]
    PathNotUtf8(PathBuf),
}

fn main() -> ExitCode {
    run(generate)
}

/// Writes the wayland-scanner macro calls into files in the output directory. The macros accept the protocol path only
/// as a string literal, and the path is known only when the build script runs.
fn generate() -> Result<(), BuildError> {
    let file = HYPRLAND_PROTOCOLS
        .path()?
        .join("share/hyprland-protocols/protocols/hyprland-focus-grab-v1.xml");
    let protocol =
        fs::canonicalize(&file).map_err(|source| BuildError::FindProtocol { file, source })?;
    println!("cargo::rerun-if-changed={}", protocol.display());
    let path = protocol
        .to_str()
        .ok_or_else(|| BuildError::PathNotUtf8(protocol.clone()))?;
    let out = output_directory()?;
    write(
        out.join("focus_grab_interfaces.rs"),
        quote! { wayland_scanner::generate_interfaces!(#path); }.to_string(),
    )?;
    Ok(write(
        out.join("focus_grab_client.rs"),
        quote! { wayland_scanner::generate_client_code!(#path); }.to_string(),
    )?)
}
