// SPDX-License-Identifier: GPL-3.0-only
use crate::fixture_error::FixtureError;
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::fs;
use std::path::Path;

pub fn read_toml<Document: DeserializeOwned>(path: &Path) -> Result<Document, FixtureError> {
    toml::from_str(&fs::read_to_string(path)?).map_err(|source| FixtureError::ReadToml {
        path: path.to_owned(),
        source,
    })
}

pub fn write_toml<Document: Serialize>(
    path: &Path,
    document: &Document,
) -> Result<(), FixtureError> {
    let text = toml::to_string(document).map_err(|source| FixtureError::WriteToml {
        path: path.to_owned(),
        source,
    })?;
    Ok(fs::write(path, text)?)
}

pub fn edit_toml<Document, Error>(
    path: &Path,
    edit: impl FnOnce(&mut Document) -> Result<(), Error>,
) -> Result<(), Error>
where
    Document: Serialize + DeserializeOwned,
    Error: From<FixtureError>,
{
    let mut document = read_toml(path)?;
    edit(&mut document)?;
    Ok(write_toml(path, &document)?)
}
