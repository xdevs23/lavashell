// SPDX-License-Identifier: GPL-3.0-only
use crate::error::Error;
use crate::files::{read_json_if_present, write_json};
use lavashell_cargo_model::lock::{CargoLock, LockedPackage};
use lavashell_cargo_model::projection::{Layer, NativeProjection, ProjectionFile};
use serde::Serialize;
use std::path::Path;

pub struct ProjectionStart<Projection> {
    pub projection: Projection,
    pub unresolved_stored_crate: Option<LockedPackage>,
}

fn start_projection<Projection: Serialize>(
    path: &Path,
    stored: Option<Projection>,
    find_unresolved_crate: impl Fn(&Projection) -> Option<&LockedPackage>,
    start_from_cargo_lock: impl FnOnce() -> Projection,
) -> Result<ProjectionStart<Projection>, Error> {
    let Some(stored) = stored else {
        return Ok(ProjectionStart {
            projection: start_from_cargo_lock(),
            unresolved_stored_crate: None,
        });
    };
    let Some(unresolved) = find_unresolved_crate(&stored).cloned() else {
        return Ok(ProjectionStart {
            projection: stored,
            unresolved_stored_crate: None,
        });
    };
    let projection = start_from_cargo_lock();
    write_json(path, &projection)?;
    Ok(ProjectionStart {
        projection,
        unresolved_stored_crate: Some(unresolved),
    })
}

pub fn start_package_projection(
    path: &Path,
    layer: Layer,
    target: &str,
    original: &CargoLock,
) -> Result<ProjectionStart<ProjectionFile>, Error> {
    let stored =
        read_json_if_present::<ProjectionFile>(path)?.map(|stored| stored.prepared(layer, target));
    start_projection(
        path,
        stored,
        |prepared| prepared.find_crate_unresolved_in(original),
        || ProjectionFile::new(original.clone()),
    )
}

pub fn start_native_projection(
    path: &Path,
    original: &CargoLock,
) -> Result<ProjectionStart<NativeProjection>, Error> {
    start_projection(
        path,
        read_json_if_present::<NativeProjection>(path)?,
        |stored| stored.find_crate_unresolved_in(original),
        || NativeProjection::new(original.clone()),
    )
}

#[cfg(test)]
mod tests {
    use super::{start_native_projection, start_package_projection};
    use crate::error::Error;
    use crate::files::{read_json, write_json};
    use lavashell_cargo_model::lock::{CargoLock, LockedPackage};
    use lavashell_cargo_model::projection::{Layer, NativeProjection, ProjectionFile};
    use std::collections::BTreeMap;
    use tempfile::TempDir;

    const TARGET: &str = "x86_64-unknown-linux-gnu";

    #[derive(Debug, thiserror::Error)]
    enum TestError {
        #[error(transparent)]
        Refresh(#[from] Error),
        #[error(transparent)]
        Directory(#[from] std::io::Error),
    }

    fn lock_of(name: &str) -> CargoLock {
        CargoLock {
            version: 4,
            package: vec![LockedPackage {
                name: name.to_owned(),
                version: "1.0.0".to_owned(),
                source: Some("registry+https://github.com/rust-lang/crates.io-index".to_owned()),
                checksum: None,
                dependencies: Vec::new(),
            }],
        }
    }

    fn engine_projection(engine_crate: &str) -> ProjectionFile {
        ProjectionFile {
            engine_lock: Some(lock_of(engine_crate)),
            features: Some(BTreeMap::new()),
            ..ProjectionFile::new(lock_of("serde"))
        }
    }

    #[test]
    fn keeps_a_registry_projection_whose_dropped_engine_lock_lists_a_removed_crate()
    -> Result<(), TestError> {
        let directory = TempDir::new()?;
        let path = directory.path().join("app.json");
        let stored = engine_projection("toml");
        write_json(&path, &stored)?;
        let start = start_package_projection(&path, Layer::Registry, TARGET, &lock_of("serde"))?;
        assert_eq!(start.projection, stored.prepared(Layer::Registry, TARGET));
        assert!(start.unresolved_stored_crate.is_none());
        assert_eq!(read_json::<ProjectionFile>(&path)?, stored);
        Ok(())
    }

    #[test]
    fn keeps_an_engine_projection_that_cargo_lock_resolves() -> Result<(), TestError> {
        let directory = TempDir::new()?;
        let path = directory.path().join("app.json");
        let stored = engine_projection("serde");
        write_json(&path, &stored)?;
        let start = start_package_projection(&path, Layer::Engine, TARGET, &lock_of("serde"))?;
        assert_eq!(start.projection, stored.prepared(Layer::Engine, TARGET));
        assert!(start.unresolved_stored_crate.is_none());
        Ok(())
    }

    #[test]
    fn restarts_and_writes_an_engine_projection_that_lists_a_removed_crate() -> Result<(), TestError>
    {
        let directory = TempDir::new()?;
        let path = directory.path().join("app.json");
        write_json(&path, &engine_projection("toml"))?;
        let start = start_package_projection(&path, Layer::Engine, TARGET, &lock_of("serde"))?;
        let restarted = ProjectionFile::new(lock_of("serde"));
        assert_eq!(start.projection, restarted);
        assert_eq!(
            start.unresolved_stored_crate.map(|package| package.name),
            Some("toml".to_owned())
        );
        assert_eq!(read_json::<ProjectionFile>(&path)?, restarted);
        Ok(())
    }

    #[test]
    fn starts_an_absent_projection_from_cargo_lock_without_writing_it() -> Result<(), TestError> {
        let directory = TempDir::new()?;
        let path = directory.path().join("app.json");
        let start = start_package_projection(&path, Layer::Registry, TARGET, &lock_of("serde"))?;
        assert_eq!(start.projection, ProjectionFile::new(lock_of("serde")));
        assert!(!path.exists());
        Ok(())
    }

    #[test]
    fn restarts_and_writes_a_native_projection_that_lists_a_removed_crate() -> Result<(), TestError>
    {
        let directory = TempDir::new()?;
        let path = directory.path().join("native.json");
        write_json(&path, &NativeProjection::new(lock_of("toml")))?;
        let start = start_native_projection(&path, &lock_of("serde"))?;
        let restarted = NativeProjection::new(lock_of("serde"));
        assert_eq!(start.projection, restarted);
        assert!(start.unresolved_stored_crate.is_some());
        assert_eq!(read_json::<NativeProjection>(&path)?, restarted);
        Ok(())
    }
}
