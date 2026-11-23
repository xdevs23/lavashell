// SPDX-License-Identifier: GPL-3.0-only
// https://doc.rust-lang.org/cargo/guide/cargo-toml-vs-cargo-lock.html
use crate::cargo_text::{CargoText, ReadByCargo};
use crate::package_source::PackageSource;
use cargo_util_schemas::lockfile::TomlLockfilePackageId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CargoLock {
    pub version: u32,
    pub package: Vec<LockedPackage>,
}

impl CargoLock {
    pub fn external_packages(&self) -> impl Iterator<Item = &LockedPackage> {
        self.package.iter().filter(|package| package.is_external())
    }

    pub fn local_packages(&self) -> impl Iterator<Item = &LockedPackage> {
        self.package.iter().filter(|package| !package.is_external())
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
    pub source: Option<PackageSource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checksum: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<LockReference>,
}

impl ReadByCargo for TomlLockfilePackageId {
    fn read(text: &str) -> Option<Self> {
        text.parse().ok()
    }
}

pub type LockReference = CargoText<TomlLockfilePackageId>;

impl LockReference {
    #[must_use]
    pub fn package_name(&self) -> Option<&str> {
        match self {
            Self::Read(id) => Some(&id.name),
            Self::Unreadable(_) => None,
        }
    }
}

impl LockedPackage {
    fn is_external(&self) -> bool {
        self.source.as_ref().is_some_and(PackageSource::is_external)
    }

    fn is_same_crate(&self, other: &LockedPackage) -> bool {
        (&self.name, &self.version, &self.source) == (&other.name, &other.version, &other.source)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::{CargoLock, LockReference, LockedPackage};
    use crate::package_source::PackageSource;

    const CRATES_IO: &str = "registry+https://github.com/rust-lang/crates.io-index";

    fn locked(name: &str, version: &str, source: Option<PackageSource>) -> LockedPackage {
        LockedPackage {
            name: name.to_owned(),
            version: version.to_owned(),
            source,
            checksum: None,
            dependencies: Vec::new(),
        }
    }

    fn lock_from(source: &str, crates: &[(&str, &str)]) -> CargoLock {
        CargoLock {
            version: 4,
            package: crates
                .iter()
                .map(|(name, version)| {
                    locked(name, version, Some(PackageSource::from(source.to_owned())))
                })
                .collect(),
        }
    }

    pub(crate) fn registry_lock(crates: &[(&str, &str)]) -> CargoLock {
        lock_from(CRATES_IO, crates)
    }

    fn unresolved(stored: &CargoLock, cargo_lock: &CargoLock) -> Option<(String, String)> {
        stored
            .find_crate_unresolved_in(cargo_lock)
            .map(|package| (package.name.clone(), package.version.clone()))
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

    #[test]
    fn matches_a_source_that_cargo_reads_as_the_same_registry() {
        let crates = [("foo", "1.0.0"), ("foo", "2.0.0")];
        assert_eq!(
            unresolved(
                &lock_from("registry+https://registry.example.org", &crates),
                &lock_from("registry+https://registry.example.org/", &crates)
            ),
            None
        );
    }

    #[test]
    fn finds_a_version_locked_from_another_registry() {
        let crates = [("foo", "1.0.0"), ("foo", "2.0.0")];
        assert_eq!(
            unresolved(
                &lock_from("registry+https://registry.example.org", &crates),
                &lock_from("sparse+https://registry.example.org/", &crates)
            ),
            Some(("foo".to_owned(), "1.0.0".to_owned()))
        );
    }

    #[test]
    fn writes_back_a_readable_reference_unchanged() -> Result<(), serde_json::Error> {
        for text in [
            "serde".to_owned(),
            "serde 1.0.229".to_owned(),
            format!("serde 1.0.229 ({CRATES_IO})"),
        ] {
            let reference = LockReference::from(text.clone());
            assert!(matches!(reference, LockReference::Read(_)), "{text}");
            assert_eq!(serde_json::to_value(&reference)?, text, "{text}");
        }
        Ok(())
    }

    #[test]
    fn writes_back_what_cargo_cannot_read_unchanged() -> Result<(), serde_json::Error> {
        let svn = "svn+https://svn.example.org/crates";
        let stored = serde_json::json!({
            "version": 4,
            "package": [
                {"name": "foo", "version": "1.0.0", "source": svn,
                 "dependencies": [format!("bar 1.0.0 ({svn})"), "serde"]},
                {"name": "serde", "version": "1.0.0", "source": CRATES_IO},
            ],
        });
        let lock: CargoLock = serde_json::from_value(stored.clone())?;
        let foo = &lock.package[0];
        assert!(matches!(foo.source, Some(PackageSource::Unreadable(_))));
        assert_eq!(
            foo.dependencies,
            [
                LockReference::from(format!("bar 1.0.0 ({svn})")),
                LockReference::from("serde".to_owned()),
            ]
        );
        assert!(matches!(
            foo.dependencies[..],
            [LockReference::Unreadable(_), LockReference::Read(_)]
        ));
        assert!(matches!(
            lock.package[1].source,
            Some(PackageSource::Read(_))
        ));
        assert_eq!(serde_json::to_value(&lock)?, stored);
        Ok(())
    }
}
