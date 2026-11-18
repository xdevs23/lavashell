// SPDX-License-Identifier: GPL-3.0-only
use std::path::PathBuf;
use std::process::ExitStatus;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("cargo metadata cannot be started in {directory}")]
    StartMetadata {
        directory: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("cargo metadata in {directory} exited with {status}")]
    Metadata {
        directory: PathBuf,
        status: ExitStatus,
    },
    #[error("the output of cargo metadata cannot be read")]
    ReadMetadata(#[source] serde_json::Error),
    #[error("{path} cannot be read")]
    ReadSource {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{path} is not the expected Cargo file")]
    ReadManifest {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
    #[error("the copy catalog {path} cannot be read")]
    ReadCopy {
        path: PathBuf,
        #[source]
        source: csv::Error,
    },
    #[error("the reader of the copy catalog {path} reported no usable line number")]
    CopyPosition { path: PathBuf },
    #[error("{path} is not valid Rust")]
    ParseSource {
        path: PathBuf,
        #[source]
        source: syn::Error,
    },
    #[error("{path} cannot be parsed")]
    ParseTree { path: PathBuf },
    #[error("the files of the repository cannot be listed")]
    ListFiles(#[source] ignore::Error),
    #[error("the {language} grammar cannot be loaded")]
    LoadGrammar {
        language: &'static str,
        #[source]
        source: tree_sitter::LanguageError,
    },
    #[error("the comment query for {language} is not valid")]
    CommentQuery {
        language: &'static str,
        #[source]
        source: tree_sitter::QueryError,
    },
    #[error("the patch {path} is not a unified diff")]
    ParsePatch {
        path: PathBuf,
        #[source]
        source: unidiff::Error,
    },
    #[error(
        "the patch {patch} changes {patched}, whose extension is not one of the languages \
         of the source rules"
    )]
    PatchedLanguage { patch: PathBuf, patched: String },
    #[error("the crate root {path} is not one of the Rust files of the repository")]
    CrateRoot { path: PathBuf },
    #[error("the crate {root} must define {name} in exactly one file")]
    Definition { root: PathBuf, name: &'static str },
    #[error("{path} has no parent directory")]
    NoParentDirectory { path: PathBuf },
    #[error("{declared_in} declares the module {module}, whose file does not exist")]
    ModuleFile {
        declared_in: PathBuf,
        module: String,
    },
}
