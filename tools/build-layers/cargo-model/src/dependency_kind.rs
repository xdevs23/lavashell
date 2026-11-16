// SPDX-License-Identifier: GPL-3.0-only
// https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use std::fmt;
use std::str::FromStr;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DependencyKind {
    Normal,
    Build,
    Dev,
}

impl DependencyKind {
    const ALL: [Self; 3] = [Self::Normal, Self::Build, Self::Dev];

    #[must_use]
    pub fn table(self) -> &'static str {
        match self {
            Self::Normal => "dependencies",
            Self::Build => "build-dependencies",
            Self::Dev => "dev-dependencies",
        }
    }
}

impl fmt::Display for DependencyKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.table())
    }
}

#[derive(Debug, thiserror::Error)]
#[error("{table} is not a table of dependencies")]
pub struct UnknownDependencyTable {
    pub table: String,
}

impl FromStr for DependencyKind {
    type Err = UnknownDependencyTable;

    fn from_str(table: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|kind| kind.table() == table)
            .ok_or_else(|| UnknownDependencyTable {
                table: table.to_owned(),
            })
    }
}

impl Serialize for DependencyKind {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.table())
    }
}

impl<'de> Deserialize<'de> for DependencyKind {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer)?
            .parse()
            .map_err(de::Error::custom)
    }
}
