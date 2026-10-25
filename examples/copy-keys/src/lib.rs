// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]
//! Generates a Rust enum of the keys in the copy catalog of an example, from the build script of that example. The
//! catalog is a TSV file of the keys and the texts shown by the example. Code that uses a key missing from the
//! catalog does not compile.

use lavashell_core::error::message_chain;
use proc_macro2::{Ident, Literal, Span, TokenStream};
use quote::quote;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::{env, fs, io};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("cargo does not set OUT_DIR")]
    OutputDirectoryNotSet,
    #[error("cannot read the copy catalog {}", .catalog.display())]
    ReadCatalog {
        catalog: PathBuf,
        source: csv::Error,
    },
    #[error("the copy key {key} is not a snake_case name of lowercase letters and digits")]
    KeyName { key: String },
    #[error("the copy key {key} gives the variant name of an earlier key")]
    DuplicateVariant { key: String },
    #[error("cannot write {}", .file.display())]
    Write { file: PathBuf, source: io::Error },
}

/// Runs in the build script of an example and writes `copy.rs` with the keys of the catalog at `catalog` to the
/// output directory of Cargo. On an error, prints the error and returns a failure exit code.
#[must_use]
pub fn build(catalog: &str) -> ExitCode {
    match generate(Path::new(catalog)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{}", message_chain(error));
            ExitCode::FAILURE
        }
    }
}

fn generate(catalog: &Path) -> Result<(), Error> {
    println!("cargo::rerun-if-changed={}", catalog.display());
    let out = env::var_os("OUT_DIR")
        .map(PathBuf::from)
        .ok_or(Error::OutputDirectoryNotSet)?;
    let code = keys(&rows(catalog)?)?;
    let file = out.join("copy.rs");
    fs::write(&file, code.to_string()).map_err(|source| Error::Write { file, source })
}

/// The key and text pairs of the catalog, a TSV file without a header or quoting.
fn rows(catalog: &Path) -> Result<Vec<(String, String)>, Error> {
    let unreadable = |source| Error::ReadCatalog {
        catalog: catalog.to_path_buf(),
        source,
    };
    csv::ReaderBuilder::new()
        .delimiter(b'\t')
        .has_headers(false)
        .quoting(false)
        .from_path(catalog)
        .map_err(unreadable)?
        .into_deserialize()
        .collect::<Result<_, _>>()
        .map_err(unreadable)
}

/// The code of the enum `CopyKey` and of the function `copy`. `CopyKey` has a variant for each row. Its `Display`
/// writes the key as it appears in the catalog. `copy(key)` returns the text of a key.
fn keys(rows: &[(String, String)]) -> Result<TokenStream, Error> {
    let mut seen = HashSet::new();
    let variants = (rows.iter())
        .map(|(key, _)| {
            let name = variant(key)?;
            if !seen.insert(name.clone()) {
                return Err(Error::DuplicateVariant { key: key.clone() });
            }
            Ok(Ident::new(&name, Span::call_site()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let names = rows.iter().map(|(key, _)| Literal::string(key));
    let texts = rows.iter().map(|(_, text)| Literal::string(text));
    Ok(quote! {
        /// A key of the copy catalog of the example.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum CopyKey {
            #(#variants,)*
        }

        impl ::std::fmt::Display for CopyKey {
            fn fmt(&self, formatter: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                formatter.write_str(match self {
                    #(Self::#variants => #names,)*
                })
            }
        }

        /// The text of `key` in the copy catalog of the example.
        #[must_use]
        pub const fn copy(key: CopyKey) -> &'static str {
            match key {
                #(CopyKey::#variants => #texts,)*
            }
        }
    })
}

/// The name of the variant of the `snake_case` `key`: its words, each with a capital first letter.
fn variant(key: &str) -> Result<String, Error> {
    let words: Vec<&str> = key.split('_').collect();
    let lowercase = |letter: char| letter.is_ascii_lowercase() || letter.is_ascii_digit();
    let snake_case = key.starts_with(|first: char| first.is_ascii_lowercase())
        && (words.iter()).all(|word| !word.is_empty() && word.chars().all(lowercase));
    if !snake_case {
        return Err(Error::KeyName {
            key: key.to_owned(),
        });
    }
    Ok(words
        .iter()
        .map(|word| {
            let (first, rest) = word.split_at(1);
            first.to_ascii_uppercase() + rest
        })
        .collect())
}

#[cfg(test)]
mod tests;
