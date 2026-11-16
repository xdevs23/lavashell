// SPDX-License-Identifier: GPL-3.0-only
// https://doc.rust-lang.org/cargo/guide/cargo-toml-vs-cargo-lock.html
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CargoLock {
    pub version: u32,
    pub package: Vec<LockedPackage>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockedPackage {
    pub name: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checksum: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<LockReference>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub struct LockReference {
    pub name: String,
    pub version: Option<String>,
    pub source: Option<String>,
}

impl From<String> for LockReference {
    fn from(text: String) -> Self {
        let mut words = text.splitn(3, ' ');
        let name = words.next().unwrap_or_default().to_owned();
        let version = words.next().map(str::to_owned);
        let source = words.next().map(|source| {
            source
                .trim_start_matches('(')
                .trim_end_matches(')')
                .to_owned()
        });
        Self {
            name,
            version,
            source,
        }
    }
}

impl fmt::Display for LockReference {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.name)?;
        if let Some(version) = &self.version {
            write!(formatter, " {version}")?;
        }
        if let Some(source) = &self.source {
            write!(formatter, " ({source})")?;
        }
        Ok(())
    }
}

impl From<LockReference> for String {
    fn from(reference: LockReference) -> Self {
        reference.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::LockReference;

    #[test]
    fn writes_back_the_reference_it_reads() {
        for text in [
            "serde",
            "serde 1.0.229",
            "serde 1.0.229 (registry+https://github.com/rust-lang/crates.io-index)",
        ] {
            assert_eq!(String::from(LockReference::from(text.to_owned())), text);
        }
    }
}
