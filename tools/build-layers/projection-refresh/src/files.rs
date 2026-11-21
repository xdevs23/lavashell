// SPDX-License-Identifier: GPL-3.0-only
use crate::error::Error;
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::fs;
use std::path::Path;

pub fn read_text(path: &Path) -> Result<String, Error> {
    fs::read_to_string(path).map_err(|source| Error::ReadFile {
        path: path.to_owned(),
        source,
    })
}

pub fn read_bytes(path: &Path) -> Result<Vec<u8>, Error> {
    fs::read(path).map_err(|source| Error::ReadFile {
        path: path.to_owned(),
        source,
    })
}

pub fn write_text(path: &Path, text: &str) -> Result<(), Error> {
    write_bytes(path, text.as_bytes())
}

pub fn write_bytes(path: &Path, contents: &[u8]) -> Result<(), Error> {
    if let Some(parent) = path.parent() {
        create_directory(parent)?;
    }
    fs::write(path, contents).map_err(|source| Error::WriteFile {
        path: path.to_owned(),
        source,
    })
}

pub fn create_directory(path: &Path) -> Result<(), Error> {
    fs::create_dir_all(path).map_err(|source| Error::WriteFile {
        path: path.to_owned(),
        source,
    })
}

pub fn read_json<Document: DeserializeOwned>(path: &Path) -> Result<Document, Error> {
    serde_json::from_str(&read_text(path)?).map_err(|source| Error::ParseJson {
        path: path.to_owned(),
        source,
    })
}

pub fn read_json_if_present<Document: DeserializeOwned>(
    path: &Path,
) -> Result<Option<Document>, Error> {
    path.exists().then(|| read_json(path)).transpose()
}

pub fn write_json<Document: Serialize>(path: &Path, document: &Document) -> Result<(), Error> {
    let text = serde_json::to_string_pretty(document).map_err(|source| Error::WriteJson {
        path: path.to_owned(),
        source,
    })?;
    write_text(path, &format!("{text}\n"))
}

pub fn read_toml<Document: DeserializeOwned>(path: &Path) -> Result<Document, Error> {
    parse_toml(path, &read_text(path)?)
}

pub fn parse_toml<Document: DeserializeOwned>(path: &Path, text: &str) -> Result<Document, Error> {
    toml::from_str(text).map_err(|source| Error::ParseToml {
        path: path.to_owned(),
        source,
    })
}

pub fn write_toml<Document: Serialize>(path: &Path, document: &Document) -> Result<(), Error> {
    let text = toml::to_string(document).map_err(|source| Error::WriteToml {
        path: path.to_owned(),
        source,
    })?;
    write_text(path, &text)
}
