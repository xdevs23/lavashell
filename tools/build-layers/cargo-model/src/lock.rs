// SPDX-License-Identifier: GPL-3.0-only
// https://doc.rust-lang.org/cargo/guide/cargo-toml-vs-cargo-lock.html
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CargoLock {
    pub version: u32,
    pub package: Vec<LockedPackage>,
}

impl CargoLock {
    pub fn external_packages(&self) -> impl Iterator<Item = &LockedPackage> {
        self.package
            .iter()
            .filter(|package| package.source.is_some())
    }

    pub fn local_packages(&self) -> impl Iterator<Item = &LockedPackage> {
        self.package
            .iter()
            .filter(|package| package.source.is_none())
    }

    fn external_packages_by_name(&self) -> BTreeMap<&str, Vec<&LockedPackage>> {
        let mut by_name: BTreeMap<&str, Vec<&LockedPackage>> = BTreeMap::new();
        for package in self.external_packages() {
            by_name.entry(&package.name).or_default().push(package);
        }
        by_name
    }

    #[must_use]
    pub fn find_crate_unresolved_in(&self, lock: &CargoLock) -> Option<&LockedPackage> {
        let stored = self.external_packages_by_name();
        let locked = lock.external_packages_by_name();
        self.external_packages().find(|package| {
            let name = package.name.as_str();
            let candidates = locked.get(name).map_or(&[][..], Vec::as_slice);
            let only_version_in_both =
                candidates.len() == 1 && stored.get(name).map_or(0, Vec::len) == 1;
            !only_version_in_both
                && !candidates
                    .iter()
                    .any(|candidate| candidate.is_same_crate(package))
        })
    }
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

impl LockedPackage {
    fn is_same_crate(&self, other: &LockedPackage) -> bool {
        (&self.name, &self.version, &self.source) == (&other.name, &other.version, &other.source)
    }
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
pub(crate) mod tests {
    use super::{CargoLock, LockReference, LockedPackage};

    const CRATES_IO: &str = "registry+https://github.com/rust-lang/crates.io-index";

    fn locked(name: &str, version: &str, source: Option<&str>) -> LockedPackage {
        LockedPackage {
            name: name.to_owned(),
            version: version.to_owned(),
            source: source.map(str::to_owned),
            checksum: None,
            dependencies: Vec::new(),
        }
    }

    pub(crate) fn registry_lock(crates: &[(&str, &str)]) -> CargoLock {
        CargoLock {
            version: 4,
            package: crates
                .iter()
                .map(|(name, version)| locked(name, version, Some(CRATES_IO)))
                .collect(),
        }
    }

    fn unresolved(stored: &CargoLock, cargo_lock: &CargoLock) -> Option<(String, String)> {
        stored
            .find_crate_unresolved_in(cargo_lock)
            .map(|package| (package.name.clone(), package.version.clone()))
    }

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

    #[test]
    fn finds_a_crate_whose_name_left_cargo_lock() {
        assert_eq!(
            unresolved(
                &registry_lock(&[("toml", "1.0.0")]),
                &registry_lock(&[("serde", "1.0.0")])
            ),
            Some(("toml".to_owned(), "1.0.0".to_owned()))
        );
    }

    #[test]
    fn ignores_a_local_crate_that_cargo_lock_lacks() {
        let stored = CargoLock {
            version: 4,
            package: vec![locked("local", "1.0.0", None)],
        };
        assert_eq!(unresolved(&stored, &registry_lock(&[])), None);
    }

    #[test]
    fn follows_the_only_version_of_a_crate_to_its_new_version() {
        assert_eq!(
            unresolved(
                &registry_lock(&[("syn", "2.0.118")]),
                &registry_lock(&[("syn", "2.0.119")])
            ),
            None
        );
    }

    #[test]
    fn finds_a_version_that_left_cargo_lock_beside_another_version() {
        assert_eq!(
            unresolved(
                &registry_lock(&[("syn", "2.0.118")]),
                &registry_lock(&[("syn", "2.0.119"), ("syn", "3.0.6")])
            ),
            Some(("syn".to_owned(), "2.0.118".to_owned()))
        );
    }

    #[test]
    fn finds_a_version_that_would_fold_onto_another_stored_version() {
        assert_eq!(
            unresolved(
                &registry_lock(&[("foo", "1.0.0"), ("foo", "2.0.0")]),
                &registry_lock(&[("foo", "2.0.0")])
            ),
            Some(("foo".to_owned(), "1.0.0".to_owned()))
        );
    }
}
