// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]

use lavashell_nix_tests::evaluation::evaluate;
use lavashell_nix_tests::fixture_error::FixtureError;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

const HOST_LOCK: &str = "packaged/.cargo-lock";
const TARGET_LOCK: &str = "x86_64-unknown-linux-gnu/packaged/.cargo-lock";
const RUSTC_INFO: &str = ".rustc_info.json";
const SHARED_OUTPUT: &str = "x86_64-unknown-linux-gnu/packaged/build/shared/output";
const READ_ONLY_DIRECTORY: u32 = 0o555;
const READ_ONLY_FILE: u32 = 0o444;
const OWNER_WRITE: u32 = 0o200;
const WRITABLE_DIRECTORY: u32 = 0o755;

#[derive(Serialize)]
struct CopyArguments<'caches> {
    caches: [&'caches Path; 2],
}

fn own_output(own: &str) -> String {
    format!("x86_64-unknown-linux-gnu/packaged/build/{own}/nested/output")
}

fn walk(directory: &Path) -> Result<Vec<PathBuf>, FixtureError> {
    let mut paths = Vec::new();
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        if path.is_dir() {
            paths.extend(walk(&path)?);
        }
        paths.push(path);
    }
    Ok(paths)
}

struct Caches {
    directory: TempDir,
    inputs: BTreeMap<PathBuf, Vec<u8>>,
}

impl Caches {
    fn new() -> Result<Self, FixtureError> {
        let directory = tempfile::tempdir()?;
        for own in ["native", "engine"] {
            let target = directory.path().join(own).join("target");
            for relative in [RUSTC_INFO, SHARED_OUTPUT, own_output(own).as_str()] {
                let path = target.join(relative);
                fs::create_dir_all(path.parent().unwrap_or(&target))?;
                fs::write(path, own)?;
            }
            let (host, cross) = (target.join(HOST_LOCK), target.join(TARGET_LOCK));
            fs::create_dir_all(host.parent().unwrap_or(&target))?;
            let _created: File = File::create(&host)?;
            fs::hard_link(&host, &cross)?;
            assert_eq!(fs::metadata(&host)?.ino(), fs::metadata(&cross)?.ino());
        }
        let mut inputs = BTreeMap::new();
        for path in walk(directory.path())? {
            let mode = if path.is_dir() {
                READ_ONLY_DIRECTORY
            } else {
                READ_ONLY_FILE
            };
            fs::set_permissions(&path, fs::Permissions::from_mode(mode))?;
            if path.is_file() {
                let _previous: Option<Vec<u8>> = inputs.insert(path.clone(), fs::read(&path)?);
            }
        }
        let caches = Self { directory, inputs };
        assert!(
            caches
                .inputs
                .keys()
                .all(|path| OpenOptions::new().write(true).open(path).is_err()),
            "the read-only caches are writable, so the copy cannot be checked against them"
        );
        Ok(caches)
    }

    fn make_directories_writable(&self) -> Result<(), FixtureError> {
        for path in walk(self.directory.path())?
            .into_iter()
            .filter(|path| path.is_dir())
        {
            fs::set_permissions(&path, fs::Permissions::from_mode(WRITABLE_DIRECTORY))?;
        }
        Ok(())
    }

    fn cache(&self, own: &str) -> PathBuf {
        self.directory.path().join(own)
    }

    fn assert_merge(&self, previous: [&str; 2], expected: &str) -> Result<(), FixtureError> {
        let [first, last] = previous.map(|own| self.cache(own));
        let script: String = evaluate(
            "cache-copy.nix",
            &CopyArguments {
                caches: [&first, &last],
            },
        )?;
        let consumer = self.directory.path().join("consumer");
        fs::create_dir(&consumer)?;
        let output = Command::new("bash")
            .args(["-eu", "-c", &script])
            .current_dir(&consumer)
            .output()?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let target = consumer.join("target");
        assert_eq!(fs::read_to_string(target.join(RUSTC_INFO))?, expected);
        assert_eq!(fs::read_to_string(target.join(SHARED_OUTPUT))?, expected);
        for own in ["native", "engine"] {
            assert_eq!(fs::read_to_string(target.join(own_output(own)))?, own);
        }
        for path in walk(&target)?.into_iter().chain([target.clone()]) {
            let mode = fs::metadata(&path)?.permissions().mode();
            assert_ne!(
                mode & OWNER_WRITE,
                0,
                "the copied target is read-only: {}",
                path.display()
            );
        }
        for (source, contents) in &self.inputs {
            assert_eq!(
                &fs::read(source)?,
                contents,
                "the source cache was modified"
            );
            assert_eq!(fs::metadata(source)?.permissions().mode() & OWNER_WRITE, 0);
        }
        assert_independent_locks(&target)
    }
}

impl Drop for Caches {
    fn drop(&mut self) {
        if let Err(error) = self.make_directories_writable() {
            eprintln!("the caches cannot be made writable for their removal: {error}");
        }
    }
}

fn assert_independent_locks(target: &Path) -> Result<(), FixtureError> {
    let (host_path, cross_path) = (target.join(HOST_LOCK), target.join(TARGET_LOCK));
    assert_ne!(
        fs::metadata(&host_path)?.ino(),
        fs::metadata(&cross_path)?.ino()
    );
    let host = OpenOptions::new().read(true).write(true).open(host_path)?;
    let cross = OpenOptions::new().read(true).write(true).open(cross_path)?;
    assert!(host.try_lock().is_ok());
    assert!(cross.try_lock_shared().is_ok());
    assert!(cross.try_lock().is_ok());
    Ok(())
}

#[test]
fn engine_overwrites_shared_files_and_adds_directories() -> Result<(), FixtureError> {
    Caches::new()?.assert_merge(["native", "engine"], "engine")
}

#[test]
fn reversed_order_keeps_last_cache_precedence() -> Result<(), FixtureError> {
    Caches::new()?.assert_merge(["engine", "native"], "native")
}
