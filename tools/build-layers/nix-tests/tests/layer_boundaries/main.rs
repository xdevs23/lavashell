// SPDX-License-Identifier: GPL-3.0-only
mod fixture;
mod manifests;

use fixture::{
    Derivation, Fixture, LayerTestError, Layers, Variant, base, bump_locked_package, bumped,
};
use lavashell_cargo_model::lock::{LockReference, LockedPackage};
use lavashell_nix_tests::toml_files::{read_toml, write_toml};
use manifests::{Inheritable, WorkspaceManifest};
use std::fs;
use std::path::{Path, PathBuf};
use toml::toml;

const CRATES_IO: &str = "registry+https://github.com/rust-lang/crates.io-index";

fn assert_unchanged(current: &Layers, layers: &[Derivation]) -> Result<(), LayerTestError> {
    let base = &base()?.layers;
    for layer in layers {
        assert_eq!(current.get(layer), base.get(layer), "{layer:?} changed");
    }
    Ok(())
}

fn assert_changed(current: &Layers, layers: &[Derivation]) -> Result<(), LayerTestError> {
    let base = &base()?.layers;
    for layer in layers {
        assert_ne!(
            current.get(layer),
            base.get(layer),
            "{layer:?} did not change"
        );
    }
    Ok(())
}

fn append_newline(path: &Path) -> Result<(), LayerTestError> {
    let source = fs::read_to_string(path)?;
    fs::write(path, source + "\n")?;
    Ok(())
}

fn reference_source(directory: &Path) -> PathBuf {
    let main = directory.join("src/main.rs");
    if main.exists() {
        main
    } else {
        directory.join("src/lib.rs")
    }
}

#[test]
fn changed_gui_tools_do_not_reach_the_engine() -> Result<(), LayerTestError> {
    let tools = Fixture::new()?.layers(Variant::GuiTools, &Derivation::ALL)?;
    assert_unchanged(&tools, &[Derivation::Engine])?;
    assert_changed(&tools, &Derivation::above_engine())
}

#[test]
fn an_edit_of_the_reference_source_reaches_only_the_reference() -> Result<(), LayerTestError> {
    let fixture = Fixture::new()?;
    append_newline(&reference_source(&fixture.root().join(&base()?.reference)))?;
    let current = fixture.all_layers()?;
    let others: Vec<_> = Derivation::ALL
        .into_iter()
        .filter(|layer| *layer != Derivation::Reference)
        .collect();
    assert_unchanged(&current, &others)?;
    assert_changed(&current, &[Derivation::Reference])
}

#[test]
fn an_edit_of_the_framework_source_leaves_the_registry_and_the_engine() -> Result<(), LayerTestError>
{
    let fixture = Fixture::new()?;
    append_newline(&fixture.root().join("crates/lavashell-core/src/lib.rs"))?;
    let current = fixture.all_layers()?;
    assert_unchanged(&current, &[Derivation::Registry, Derivation::Engine])?;
    assert_unchanged(&current, &Derivation::OFF_FRAMEWORK)?;
    assert_changed(
        &current,
        &[
            Derivation::Framework,
            Derivation::ReferenceFramework,
            Derivation::Reference,
            Derivation::Native,
        ],
    )
}

#[test]
fn an_edit_of_the_error_chain_changes_the_packages_off_the_framework() -> Result<(), LayerTestError>
{
    let fixture = Fixture::new()?;
    append_newline(
        &fixture
            .root()
            .join("crates/lavashell-error-chain/src/lib.rs"),
    )?;
    let current = fixture.all_layers()?;
    assert_unchanged(&current, &[Derivation::Registry, Derivation::Engine])?;
    assert_changed(&current, &Derivation::OFF_FRAMEWORK)
}

#[test]
fn an_unrelated_example_and_its_registry_update_reach_no_layer() -> Result<(), LayerTestError> {
    let fixture = Fixture::new()?;
    let foreign = fixture.root().join("examples/foreign-fixture");
    let manifest = toml! {
        [package]
        name = "foreign-fixture"
        version = "0.1.0"
        edition = "2024"

        [dependencies]
        foreign-registry-fixture = "=1.0.0"
    };
    fs::create_dir_all(foreign.join("src"))?;
    fs::write(foreign.join("src/main.rs"), "fn main() {}\n")?;
    write_toml(&foreign.join("Cargo.toml"), &manifest)?;
    fixture.edit_workspace(|manifest| {
        manifest
            .workspace
            .members
            .push("examples/foreign-fixture".to_owned());
        Ok(())
    })?;
    fixture.edit_lock(|lock| {
        lock.package.extend([
            LockedPackage {
                name: "foreign-fixture".to_owned(),
                version: "0.1.0".to_owned(),
                source: None,
                checksum: None,
                dependencies: vec![LockReference::from("foreign-registry-fixture".to_owned())],
            },
            LockedPackage {
                name: "foreign-registry-fixture".to_owned(),
                version: "1.0.0".to_owned(),
                source: Some(CRATES_IO.to_owned()),
                checksum: Some("0".repeat(64)),
                dependencies: Vec::new(),
            },
        ]);
        Ok(())
    })?;
    assert_eq!(&fixture.all_layers()?, &base()?.layers);
    fixture.edit_lock(|lock| {
        bump_locked_package(lock, "foreign-registry-fixture", "1.0.0", "1.0.1")
    })?;
    assert_eq!(&fixture.all_layers()?, &base()?.layers);
    Ok(())
}

#[test]
fn a_registry_update_of_gui_crates_makes_the_projections_stale_and_leaves_the_engine()
-> Result<(), LayerTestError> {
    let fixture = Fixture::new()?;
    fixture.bump_workspace_dependency("zbus")?;
    assert_eq!(fixture.stale_layers()?, Derivation::above_engine());
    assert_unchanged(
        &fixture.layers(Variant::Plain, &[Derivation::Engine])?,
        &[Derivation::Engine],
    )
}

#[test]
fn an_added_engine_feature_makes_the_projections_stale_and_leaves_the_other_layers()
-> Result<(), LayerTestError> {
    let fixture = Fixture::new()?;
    fixture.edit_member(&base()?.reference, |manifest| {
        let features = manifest
            .dependencies
            .get_mut("bevy")
            .and_then(manifests::Dependency::features_mut)
            .ok_or_else(|| LayerTestError::MissingDependency {
                name: "bevy".to_owned(),
            })?;
        features.push("bevy_scene".to_owned());
        Ok(())
    })?;
    assert_eq!(fixture.stale_layers()?, Derivation::ON_ENGINE);
    assert_unchanged(
        &fixture.layers(Variant::Plain, &Derivation::OFF_ENGINE)?,
        &Derivation::OFF_ENGINE,
    )
}

#[test]
fn a_version_change_of_the_adapter_reaches_only_the_reference_layers() -> Result<(), LayerTestError>
{
    let fixture = Fixture::new()?;
    let workspace: WorkspaceManifest = read_toml(&fixture.root().join("Cargo.toml"))?;
    let old = workspace.workspace.package.version;
    let new = bumped(&old)?;
    let mut adapter = String::new();
    fixture.edit_member(&base()?.reference, |manifest| {
        manifest.package.name.clone_into(&mut adapter);
        manifest.package.version = Inheritable::Value(new.clone());
        Ok(())
    })?;
    fixture.edit_lock(|lock| bump_locked_package(lock, &adapter, &old, &new))?;
    let current = fixture.all_layers()?;
    assert_unchanged(
        &current,
        &[
            Derivation::Registry,
            Derivation::Framework,
            Derivation::Engine,
            Derivation::Native,
        ],
    )?;
    assert_changed(
        &current,
        &[Derivation::ReferenceFramework, Derivation::Reference],
    )
}

#[test]
fn a_version_change_of_the_engine_makes_the_projections_stale_and_leaves_the_other_layers()
-> Result<(), LayerTestError> {
    let fixture = Fixture::new()?;
    fixture.bump_workspace_dependency("bevy")?;
    assert_eq!(fixture.stale_layers()?, Derivation::ON_ENGINE);
    assert_unchanged(
        &fixture.layers(Variant::Plain, &Derivation::OFF_ENGINE)?,
        &Derivation::OFF_ENGINE,
    )
}
