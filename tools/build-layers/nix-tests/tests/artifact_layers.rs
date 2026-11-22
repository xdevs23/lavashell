// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]

use lavashell_nix_tests::evaluation::evaluate;
use lavashell_nix_tests::file_tree::walk;
use lavashell_nix_tests::fixture_error::FixtureError;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, UNIX_EPOCH};
use tempfile::TempDir;

const HOST_LOCK: &str = "packaged/.cargo-lock";
const TARGET_LOCK: &str = "x86_64-unknown-linux-gnu/packaged/.cargo-lock";
const RUSTC_INFO: &str = ".rustc_info.json";
const SHARED_OUTPUT: &str = "x86_64-unknown-linux-gnu/packaged/build/shared/output";
const BUILD_SCRIPT: &str =
    "packaged/build/ash-87eb77ff61665c87/build_script_build-87eb77ff61665c87";
const UPLIFTED_BUILD_SCRIPT: &str = "packaged/build/ash-87eb77ff61665c87/build-script-build";
const KEPT_OUTPUT: &str = "packaged/deps/libkept.rlib";
const REMOVED_OUTPUT: &str = "packaged/deps/libdummy.rlib";
const ADDED_OUTPUT: &str = "packaged/deps/libadded.rlib";
const ADDED_LINK: &str = "packaged/libadded.rlib";
const EMPTY_DIRECTORY: &str = "packaged/incremental";
const REMOVED_PATHS: &str = "share/lavashell/removed-target-paths";
const STORE_MODIFICATION_TIME_SINCE_EPOCH: Duration = Duration::from_secs(1);
const READ_ONLY_DIRECTORY: u32 = 0o555;
const READ_ONLY_FILE: u32 = 0o444;
const OWNER_WRITE: u32 = 0o200;
const WRITABLE_DIRECTORY: u32 = 0o755;

#[derive(Serialize)]
struct Layer<'stack> {
    output: PathBuf,
    previous: &'stack [&'stack str],
}

#[derive(Serialize)]
struct StackArguments<'stack> {
    layers: BTreeMap<&'stack str, Layer<'stack>>,
    restored: &'stack [&'stack str],
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LayerScripts {
    stack: Vec<String>,
    restore: String,
    restore_lower_layers: String,
    store_layer_changes: String,
}

fn own_output(own: &str) -> String {
    format!("x86_64-unknown-linux-gnu/packaged/build/{own}/nested/output")
}

fn files(target: &Path) -> Result<BTreeMap<PathBuf, Vec<u8>>, FixtureError> {
    let mut contents = BTreeMap::new();
    for path in walk(target)?.into_iter().filter(|path| path.is_file()) {
        let relative = path.strip_prefix(target).unwrap_or(&path).to_owned();
        let _previous: Option<Vec<u8>> = contents.insert(relative, fs::read(&path)?);
    }
    Ok(contents)
}

struct LayerOutputs {
    directory: TempDir,
}

impl LayerOutputs {
    fn new() -> Result<Self, FixtureError> {
        Ok(Self {
            directory: tempfile::tempdir()?,
        })
    }

    fn path(&self, name: &str) -> PathBuf {
        self.directory.path().join(name)
    }

    fn add_layer(&self, name: &str, files: &[(&str, &str)]) -> Result<(), FixtureError> {
        let output = self.path(name);
        let target = output.join("target");
        fs::create_dir_all(&target)?;
        for (relative, contents) in files {
            let path = target.join(relative);
            fs::create_dir_all(path.parent().unwrap_or(&target))?;
            let mut file = File::create(path)?;
            file.write_all(contents.as_bytes())?;
            file.set_modified(UNIX_EPOCH + STORE_MODIFICATION_TIME_SINCE_EPOCH)?;
        }
        let removed = output.join(REMOVED_PATHS);
        fs::create_dir_all(removed.parent().unwrap_or(&output))?;
        let _created: File = File::create(removed)?;
        Ok(())
    }

    fn add_linked_locks(&self, name: &str) -> Result<(), FixtureError> {
        let target = self.path(name).join("target");
        let (host, cross) = (target.join(HOST_LOCK), target.join(TARGET_LOCK));
        fs::create_dir_all(host.parent().unwrap_or(&target))?;
        let _created: File = File::create(&host)?;
        fs::hard_link(&host, &cross)?;
        assert_eq!(fs::metadata(&host)?.ino(), fs::metadata(&cross)?.ino());
        Ok(())
    }

    fn seal(&self) -> Result<BTreeMap<PathBuf, Vec<u8>>, FixtureError> {
        let mut inputs = BTreeMap::new();
        for path in walk(self.directory.path())? {
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
        assert!(
            inputs
                .keys()
                .all(|path| OpenOptions::new().write(true).open(path).is_err()),
            "the read-only layer outputs are writable, so the restore cannot be checked against them"
        );
        Ok(inputs)
    }

    fn scripts(
        &self,
        layers: &[(&str, &[&str])],
        restored: &[&str],
    ) -> Result<LayerScripts, FixtureError> {
        let layers = layers
            .iter()
            .map(|&(name, previous)| {
                let output = self.path(name);
                (name, Layer { output, previous })
            })
            .collect();
        evaluate("artifact-layers.nix", &StackArguments { layers, restored })
    }

    fn run(&self, script: &str, directory: &str) -> Result<PathBuf, FixtureError> {
        let directory = self.path(directory);
        fs::create_dir(&directory)?;
        self.run_in(&directory, script, &[])?;
        Ok(directory)
    }

    fn run_in(
        &self,
        directory: &Path,
        script: &str,
        environment: &[(&str, &Path)],
    ) -> Result<(), FixtureError> {
        let output = Command::new("bash")
            .args(["-euo", "pipefail", "-c", script])
            .current_dir(directory)
            .env("NIX_BUILD_TOP", self.directory.path())
            .envs(environment.iter().copied())
            .output()?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(())
    }

    fn build_layer(
        &self,
        scripts: &LayerScripts,
        name: &str,
        cargo: impl FnOnce(&Path) -> Result<(), FixtureError>,
    ) -> Result<PathBuf, FixtureError> {
        let build = self.path("build");
        fs::create_dir(&build)?;
        self.run_in(&build, &scripts.restore_lower_layers, &[])?;
        cargo(&build.join("target"))?;
        self.run_in(
            &build,
            &scripts.store_layer_changes,
            &[("out", self.path(name).as_path())],
        )?;
        Ok(build.join("target"))
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
}

impl Drop for LayerOutputs {
    fn drop(&mut self) {
        if let Err(error) = self.make_directories_writable() {
            eprintln!("the layer outputs cannot be made writable for their removal: {error}");
        }
    }
}

fn assert_writable(target: &Path) -> Result<(), FixtureError> {
    for path in walk(target)?.into_iter().chain([target.to_owned()]) {
        let mode = fs::metadata(&path)?.permissions().mode();
        assert_ne!(
            mode & OWNER_WRITE,
            0,
            "the restored target is read-only: {}",
            path.display()
        );
    }
    Ok(())
}

fn assert_unchanged(inputs: &BTreeMap<PathBuf, Vec<u8>>) -> Result<(), FixtureError> {
    for (source, contents) in inputs {
        assert_eq!(
            &fs::read(source)?,
            contents,
            "the layer output was modified"
        );
        assert_eq!(fs::metadata(source)?.permissions().mode() & OWNER_WRITE, 0);
    }
    Ok(())
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

fn assert_independent_merge(restored: [&str; 2], expected: &str) -> Result<(), FixtureError> {
    let outputs = LayerOutputs::new()?;
    for own in ["native", "engine"] {
        let own_path = own_output(own);
        outputs.add_layer(
            own,
            &[(RUSTC_INFO, own), (SHARED_OUTPUT, own), (&own_path, own)],
        )?;
        outputs.add_linked_locks(own)?;
    }
    let inputs = outputs.seal()?;
    let scripts = outputs.scripts(&[("native", &[]), ("engine", &[])], &restored)?;
    let target = outputs.run(&scripts.restore, "consumer")?.join("target");
    assert_eq!(fs::read_to_string(target.join(RUSTC_INFO))?, expected);
    assert_eq!(fs::read_to_string(target.join(SHARED_OUTPUT))?, expected);
    for own in ["native", "engine"] {
        assert_eq!(fs::read_to_string(target.join(own_output(own)))?, own);
    }
    assert_writable(&target)?;
    assert_unchanged(&inputs)?;
    assert_independent_locks(&target)
}

#[test]
fn engine_overwrites_shared_files_and_adds_directories() -> Result<(), FixtureError> {
    assert_independent_merge(["native", "engine"], "engine")
}

#[test]
fn reversed_order_keeps_last_cache_precedence() -> Result<(), FixtureError> {
    assert_independent_merge(["engine", "native"], "native")
}

#[test]
fn shared_lower_layer_restored_again_would_overwrite_what_a_layer_on_it_rewrote()
-> Result<(), FixtureError> {
    let outputs = LayerOutputs::new()?;
    let library_path = own_output("library");
    outputs.add_layer(
        "framework",
        &[(RUSTC_INFO, "framework"), (SHARED_OUTPUT, "framework")],
    )?;
    outputs.add_layer("local", &[(RUSTC_INFO, "local")])?;
    outputs.add_layer("library", &[(&library_path, "library")])?;
    let inputs = outputs.seal()?;
    let scripts = outputs.scripts(
        &[
            ("framework", &[]),
            ("local", &["framework"]),
            ("library", &["framework"]),
        ],
        &["local", "library"],
    )?;
    assert_eq!(scripts.stack, ["framework", "local", "library"]);
    let target = outputs.run(&scripts.restore, "consumer")?.join("target");
    assert_eq!(fs::read_to_string(target.join(RUSTC_INFO))?, "local");
    assert_eq!(fs::read_to_string(target.join(SHARED_OUTPUT))?, "framework");
    assert_eq!(fs::read_to_string(target.join(library_path))?, "library");
    assert_writable(&target)?;
    assert_unchanged(&inputs)
}

fn build_layer_on_framework(outputs: &LayerOutputs) -> Result<PathBuf, FixtureError> {
    outputs.add_layer(
        "framework",
        &[
            (RUSTC_INFO, "framework"),
            (BUILD_SCRIPT, "build script"),
            (UPLIFTED_BUILD_SCRIPT, "build script"),
            (KEPT_OUTPUT, "kept"),
            (REMOVED_OUTPUT, "dummy"),
        ],
    )?;
    let _inputs = outputs.seal()?;
    let scripts = outputs.scripts(&[("framework", &[])], &["framework"])?;
    outputs.build_layer(&scripts, "layer", |target| {
        fs::write(target.join(RUSTC_INFO), "layer")?;
        fs::remove_file(target.join(UPLIFTED_BUILD_SCRIPT))?;
        fs::hard_link(
            target.join(BUILD_SCRIPT),
            target.join(UPLIFTED_BUILD_SCRIPT),
        )?;
        fs::write(target.join(ADDED_OUTPUT), "added")?;
        fs::hard_link(target.join(ADDED_OUTPUT), target.join(ADDED_LINK))?;
        fs::remove_file(target.join(REMOVED_OUTPUT))?;
        fs::create_dir_all(target.join(EMPTY_DIRECTORY))?;
        Ok(())
    })
}

#[test]
fn layer_stores_what_it_wrote_but_not_a_relinked_lower_file_whose_change_time_moved()
-> Result<(), FixtureError> {
    let outputs = LayerOutputs::new()?;
    let _built: PathBuf = build_layer_on_framework(&outputs)?;
    let layer = outputs.path("layer");
    let stored = layer.join("target");
    assert_eq!(
        files(&stored)?,
        BTreeMap::from([
            (PathBuf::from(RUSTC_INFO), b"layer".to_vec()),
            (PathBuf::from(ADDED_OUTPUT), b"added".to_vec()),
            (PathBuf::from(ADDED_LINK), b"added".to_vec()),
        ])
    );
    assert_eq!(
        fs::metadata(stored.join(ADDED_OUTPUT))?.ino(),
        fs::metadata(stored.join(ADDED_LINK))?.ino(),
        "a hard link pair of the layer is stored as two files"
    );
    assert!(!stored.join(EMPTY_DIRECTORY).exists());
    assert_eq!(
        fs::read(layer.join(REMOVED_PATHS))?,
        format!("{REMOVED_OUTPUT}\0").into_bytes()
    );
    Ok(())
}

#[test]
fn replaying_the_stack_rebuilds_the_layer_target_without_the_files_it_removed()
-> Result<(), FixtureError> {
    let outputs = LayerOutputs::new()?;
    let built = build_layer_on_framework(&outputs)?;
    let scripts = outputs.scripts(&[("framework", &[]), ("layer", &["framework"])], &["layer"])?;
    let replayed = outputs.run(&scripts.restore, "consumer")?.join("target");
    assert_eq!(files(&replayed)?, files(&built)?);
    assert!(!replayed.join(REMOVED_OUTPUT).exists());
    Ok(())
}
