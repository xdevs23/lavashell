// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]

mod fixture;
mod manifests;

use fixture::{
    Derivation, Fixture, LayerDerivation, LayerTestError, Layers, Variant, base,
    bump_locked_package, bumped,
};
use lavashell_cargo_model::lock::{LockReference, LockedPackage};
use lavashell_nix_tests::toml_files::{read_toml, write_toml};
use manifests::{Inheritable, WorkspaceManifest};
use std::fs;
use std::path::{Path, PathBuf};
use toml::{Table, toml};

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
fn changed_gui_tools_leave_every_engine_layer() -> Result<(), LayerTestError> {
    let tools = Fixture::new()?.layers(Variant::GuiTools, &base()?.all())?;
    let engines = base()?.engines();
    assert_unchanged(&tools, &engines)?;
    assert_changed(&tools, &base()?.all_except(&engines))
}

#[test]
fn an_edit_of_the_reference_source_reaches_only_the_reference() -> Result<(), LayerTestError> {
    let fixture = Fixture::new()?;
    append_newline(&reference_source(&fixture.root().join(&base()?.reference)))?;
    let current = fixture.all_layers()?;
    assert_unchanged(
        &current,
        &base()?.all_except(&[Derivation::Layer(LayerDerivation::Reference)]),
    )?;
    assert_changed(&current, &[Derivation::Layer(LayerDerivation::Reference)])
}

#[test]
fn an_edit_of_the_recordings_source_changes_only_its_library_layer_and_package()
-> Result<(), LayerTestError> {
    let fixture = Fixture::new()?;
    append_newline(&fixture.root().join("examples/recordings/src/lib.rs"))?;
    let current = fixture.all_layers()?;
    let recordings = base()?.packages.crate_with_engine("recordings")?;
    let built_from_its_source = [recordings.framework.clone(), recordings.package.clone()];
    assert_unchanged(&current, &base()?.all_except(&built_from_its_source))?;
    assert_changed(&current, &built_from_its_source)
}

#[test]
fn an_edit_of_the_test_platform_source_changes_only_the_packages_built_on_it()
-> Result<(), LayerTestError> {
    let fixture = Fixture::new()?;
    append_newline(
        &fixture
            .root()
            .join("crates/lavashell-test-platform/src/lib.rs"),
    )?;
    let current = fixture.all_layers()?;
    let packages = &base()?.packages;
    let recordings = packages.crate_with_engine("recordings")?;
    let built_on_it = [
        packages.registry_only_crate("test-platform")?.clone(),
        recordings.framework.clone(),
        recordings.package.clone(),
    ];
    assert_unchanged(&current, &base()?.all_except(&built_on_it))?;
    assert_changed(&current, &built_on_it)
}

#[test]
fn an_edit_of_the_framework_source_leaves_the_registry_the_engine_and_the_unified_layers()
-> Result<(), LayerTestError> {
    let fixture = Fixture::new()?;
    append_newline(&fixture.root().join("crates/lavashell-core/src/lib.rs"))?;
    let current = fixture.all_layers()?;
    let packages = &base()?.packages;
    let test_platform = packages.registry_only_crate("test-platform")?;
    let without_framework_source: Vec<Derivation> = (packages.registry_only.iter())
        .filter(|package| *package != test_platform)
        .cloned()
        .collect();
    assert_unchanged(&current, &[Derivation::Layer(LayerDerivation::Registry)])?;
    assert_unchanged(&current, &base()?.engines())?;
    assert_unchanged(&current, &base()?.unified_layers())?;
    assert_unchanged(&current, &without_framework_source)?;
    assert_changed(
        &current,
        &[
            Derivation::Layer(LayerDerivation::Framework),
            Derivation::Layer(LayerDerivation::ReferenceFramework),
            Derivation::Layer(LayerDerivation::Reference),
            Derivation::Layer(LayerDerivation::Native),
            test_platform.clone(),
        ],
    )
}

#[test]
fn an_edit_of_the_framework_source_changes_the_packaged_crates_on_the_framework()
-> Result<(), LayerTestError> {
    let fixture = Fixture::new()?;
    append_newline(&fixture.root().join("crates/lavashell-core/src/lib.rs"))?;
    let on_framework = base()?.packages.built_on_framework();
    assert_changed(
        &fixture.layers(Variant::Plain, &on_framework)?,
        &on_framework,
    )
}

#[test]
fn an_edit_of_the_error_chain_changes_the_packages_on_and_off_the_framework()
-> Result<(), LayerTestError> {
    let fixture = Fixture::new()?;
    append_newline(
        &fixture
            .root()
            .join("crates/lavashell-error-chain/src/lib.rs"),
    )?;
    let current = fixture.all_layers()?;
    assert_unchanged(
        &current,
        &[
            Derivation::Layer(LayerDerivation::Registry),
            Derivation::Layer(LayerDerivation::Engine),
        ],
    )?;
    assert_changed(&current, &base()?.packages.registry_only)?;
    assert_changed(&current, &base()?.packages.built_on_framework())
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
fn an_edit_of_the_nix_tests_reaches_no_layer() -> Result<(), LayerTestError> {
    let fixture = Fixture::new()?;
    let nix_tests = fixture.root().join("tools/build-layers/nix-tests");
    append_newline(&nix_tests.join("src/lib.rs"))?;
    append_newline(&nix_tests.join("expressions/layer-derivations.nix"))?;
    assert_eq!(&fixture.all_layers()?, &base()?.layers);
    Ok(())
}

#[test]
fn an_edit_of_the_nix_tests_reaches_the_layers_once_a_tool_depends_on_them()
-> Result<(), LayerTestError> {
    let fixture = Fixture::new()?;
    let tools = fixture.root().join("tools/build-layers");
    let tool_manifest = tools.join("plan-check/Cargo.toml");
    let mut manifest: Table = read_toml(&tool_manifest)?;
    let _absent_before: Option<toml::Value> = manifest.insert(
        "dev-dependencies".to_owned(),
        toml! { lavashell-nix-tests = { workspace = true } }.into(),
    );
    write_toml(&tool_manifest, &manifest)?;
    let depending = fixture.all_layers()?;
    append_newline(&tools.join("nix-tests/src/lib.rs"))?;
    assert_ne!(&fixture.all_layers()?, &depending);
    Ok(())
}

#[test]
fn a_registry_update_of_gui_crates_makes_the_projections_stale_and_leaves_the_engine()
-> Result<(), LayerTestError> {
    let fixture = Fixture::new()?;
    fixture.bump_workspace_dependency("zbus")?;
    let engines = base()?.engines();
    assert_eq!(fixture.stale_layers()?, base()?.all_except(&engines));
    assert_unchanged(&fixture.layers(Variant::Plain, &engines)?, &engines)
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
    let on_engine = Derivation::layers_on_engine();
    assert_eq!(fixture.stale_layers()?, on_engine);
    let off_engine = base()?.all_except(&on_engine);
    assert_unchanged(&fixture.layers(Variant::Plain, &off_engine)?, &off_engine)
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
    let reference_layers = [
        Derivation::Layer(LayerDerivation::Unified),
        Derivation::Layer(LayerDerivation::ReferenceFramework),
        Derivation::Layer(LayerDerivation::Reference),
    ];
    assert_unchanged(&current, &base()?.all_except(&reference_layers))?;
    assert_changed(&current, &reference_layers)
}

#[test]
fn a_version_change_of_the_engine_makes_the_projections_stale_and_leaves_the_other_layers()
-> Result<(), LayerTestError> {
    let fixture = Fixture::new()?;
    fixture.bump_workspace_dependency("bevy")?;
    let on_engine = Derivation::layers_on_engine();
    assert_eq!(fixture.stale_layers()?, on_engine);
    let off_engine = base()?.all_except(&on_engine);
    assert_unchanged(&fixture.layers(Variant::Plain, &off_engine)?, &off_engine)
}
