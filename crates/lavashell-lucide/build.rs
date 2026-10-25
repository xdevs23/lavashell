// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]

mod convert;
mod outline;

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::{fs, io};

use convert::{ConvertError, convert};
use lavashell_build_script::{BuildScriptError, PackageVariable, output_directory, run, write};
use proc_macro2::{Ident, Literal, Span, TokenStream};
use quote::quote;
use thiserror::Error;

const LUCIDE: PackageVariable = PackageVariable {
    name: "LAVASHELL_LUCIDE",
    package: "the lucide-static package of this flake",
};

#[derive(Debug, Error)]
enum BuildError {
    #[error(transparent)]
    Script(#[from] BuildScriptError),
    #[error("cannot list the icons in {}", .directory.display())]
    ListIcons {
        directory: PathBuf,
        source: io::Error,
    },
    #[error("the icon {} does not have a name in UTF-8", .file.display())]
    IconName { file: PathBuf },
    #[error("cannot read the icon {}", .file.display())]
    ReadIcon { file: PathBuf, source: io::Error },
    #[error("cannot convert the icon {}", .file.display())]
    ConvertIcon { file: PathBuf, source: ConvertError },
    #[error("cannot encode the icon {}", .file.display())]
    EncodeIcon {
        file: PathBuf,
        source: postcard::Error,
    },
}

fn main() -> ExitCode {
    run(generate)
}

fn generate() -> Result<(), BuildError> {
    let icons = LUCIDE.path()?.join("icons");
    println!("cargo::rerun-if-changed={}", icons.display());
    let out = output_directory()?;
    let statics = svg_files(&icons)?
        .iter()
        .map(|file| icon(file, &out))
        .collect::<Result<Vec<_>, _>>()?;
    let code = quote! { #(#statics)* };
    Ok(write(out.join("icons.rs"), code.to_string())?)
}

/// The SVG files in `directory`, sorted by name so that the generated code is the same in every build.
fn svg_files(directory: &Path) -> Result<Vec<PathBuf>, BuildError> {
    let list = |source| BuildError::ListIcons {
        directory: directory.to_path_buf(),
        source,
    };
    let mut files = Vec::new();
    for entry in fs::read_dir(directory).map_err(list)? {
        let path = entry.map_err(list)?.path();
        if path.extension().is_some_and(|extension| extension == "svg") {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

/// Converts the SVG icon in `file` and writes it, encoded with postcard, to a file in `out`. Returns the
/// code of a static that decodes that file the first time the icon is used.
fn icon(file: &Path, out: &Path) -> Result<TokenStream, BuildError> {
    let stem =
        (file.file_stem().and_then(|stem| stem.to_str())).ok_or_else(|| BuildError::IconName {
            file: file.to_path_buf(),
        })?;
    let svg = fs::read_to_string(file).map_err(|source| BuildError::ReadIcon {
        file: file.to_path_buf(),
        source,
    })?;
    let converted = convert(&svg).map_err(|source| BuildError::ConvertIcon {
        file: file.to_path_buf(),
        source,
    })?;
    let encoded = postcard::to_allocvec(&converted).map_err(|source| BuildError::EncodeIcon {
        file: file.to_path_buf(),
        source,
    })?;
    let data = format!("{stem}.bin");
    write(out.join(&data), encoded)?;
    let name = Ident::new(&stem.replace('-', "_").to_uppercase(), Span::call_site());
    let data = Literal::string(&format!("/{data}"));
    let stem = Literal::string(stem);
    Ok(quote! {
        pub static #name: EncodedIcon =
            EncodedIcon(LazyLock::new(|| icon(#stem, include_bytes!(concat!(env!("OUT_DIR"), #data)))));
    })
}
