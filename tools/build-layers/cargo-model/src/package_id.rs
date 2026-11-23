// SPDX-License-Identifier: GPL-3.0-only
// https://doc.rust-lang.org/cargo/reference/pkgid-spec.html
use crate::package_source::PackageSource;
use serde::{Deserialize, Serialize};
use std::fmt;
use url::Url;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub struct PackageId {
    text: String,
    source: Option<PackageSource>,
    name: Option<String>,
    version: Option<String>,
}

impl PackageId {
    #[must_use]
    pub fn is_external(&self) -> bool {
        self.source.as_ref().is_some_and(PackageSource::is_external)
    }

    #[must_use]
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    #[must_use]
    pub fn version(&self) -> Option<&str> {
        self.version.as_deref()
    }
}

impl From<String> for PackageId {
    fn from(text: String) -> Self {
        let (location, fragment) = text.split_once('#').unwrap_or((&text, ""));
        let source = location
            .contains('+')
            .then(|| PackageSource::from(location.to_owned()));
        let (name, version) = match fragment.rsplit_once('@') {
            Some((name, version)) => (Some(name.to_owned()), Some(version.to_owned())),
            None => (
                last_path_segment(location),
                Some(fragment.to_owned()).filter(|version| !version.is_empty()),
            ),
        };
        Self {
            text,
            source,
            name,
            version,
        }
    }
}

fn last_path_segment(location: &str) -> Option<String> {
    let (_source_kind, address) = location.split_once('+')?;
    let url = Url::parse(address).ok()?;
    url.path_segments()?
        .rfind(|segment| !segment.is_empty())
        .map(str::to_owned)
}

impl From<PackageId> for String {
    fn from(id: PackageId) -> Self {
        id.text
    }
}

impl fmt::Display for PackageId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.text)
    }
}

#[cfg(test)]
mod tests {
    use super::PackageId;
    use crate::package_source::PackageSource;
    use cargo_util_schemas::core::{GitReference, SourceKind};

    fn package_id(text: &str) -> PackageId {
        PackageId::from(text.to_owned())
    }

    #[test]
    fn counts_every_source_but_a_path_as_external() {
        let cases = [
            (
                "registry+https://github.com/rust-lang/crates.io-index#anstream@1.0.0",
                SourceKind::Registry,
                true,
            ),
            (
                "sparse+https://index.crates.io/#serde@1.0.229",
                SourceKind::SparseRegistry,
                true,
            ),
            (
                "git+https://github.com/rust-skia/skia-bindings?rev=abc#0.82.0",
                SourceKind::Git(GitReference::Rev("abc".to_owned())),
                true,
            ),
            (
                "path+file:///work/crates/lavashell-core#0.1.0",
                SourceKind::Path,
                false,
            ),
        ];
        for (text, kind, external) in cases {
            let id = package_id(text);
            assert!(
                matches!(&id.source, Some(PackageSource::Read(source)) if *source.kind() == kind),
                "{text}"
            );
            assert_eq!(id.is_external(), external, "{text}");
        }
    }

    #[test]
    fn marks_local_registry_and_directory_sources_unreadable_and_external() {
        for text in [
            "local-registry+file:///work/registry#serde@1.0.229",
            "directory+file:///work/vendor#serde@1.0.229",
        ] {
            let id = package_id(text);
            assert!(
                matches!(id.source, Some(PackageSource::Unreadable(_))),
                "{text}"
            );
            assert!(id.is_external(), "{text}");
            assert_eq!(
                (id.name(), id.version()),
                (Some("serde"), Some("1.0.229")),
                "{text}"
            );
        }
    }

    #[test]
    fn keeps_a_source_kind_unknown_to_cargo_and_counts_it_external() {
        let unknown = package_id("svn+https://example.org/repository#serde@1.0.229");
        assert!(matches!(unknown.source, Some(PackageSource::Unreadable(_))));
        assert!(unknown.is_external());
        assert_eq!(
            String::from(unknown),
            "svn+https://example.org/repository#serde@1.0.229"
        );
    }

    #[test]
    fn counts_an_id_without_a_source_as_local() {
        let id = package_id("serde@1.0.229");
        assert_eq!(id.source, None);
        assert!(!id.is_external());
    }

    #[test]
    fn reads_the_name_and_version() {
        let named =
            package_id("registry+https://github.com/rust-lang/crates.io-index#anstream@1.0.0");
        let unnamed = package_id("git+https://github.com/rust-skia/skia-bindings?rev=abc#0.82.0");
        assert_eq!(
            (named.name(), named.version()),
            (Some("anstream"), Some("1.0.0"))
        );
        assert_eq!(
            (unnamed.name(), unnamed.version()),
            (Some("skia-bindings"), Some("0.82.0"))
        );
    }
}
