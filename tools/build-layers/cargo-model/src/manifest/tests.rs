// SPDX-License-Identifier: GPL-3.0-only
use super::{BuildScript, FeatureActivations, RequestSource, StubManifest};
use crate::dependency_kind::DependencyKind;
use serde_json::{Value, json};

fn stored_manifest() -> Value {
    json!({
        "build-dependencies": {
            "syn": {
                "default-features": false, "features": ["derive", "full"],
                "package": "syn", "version": "=3.0.6"
            }
        },
        "dependencies": {
            "support": {
                "default-features": false, "features": [],
                "package": "lavashell-example-support", "path": "../support"
            }
        },
        "features": { "trace": [] },
        "lib": { "path": "src/lib.rs", "proc-macro": true },
        "package": {
            "autobenches": false, "autobins": false, "autoexamples": false, "autotests": false,
            "build": "build.rs", "edition": "2024", "name": "lavashell-music-player",
            "version": "0.0.0"
        }
    })
}

#[test]
fn reads_a_stored_stub_manifest() -> Result<(), serde_json::Error> {
    let manifest: StubManifest = serde_json::from_value(stored_manifest())?;
    assert_eq!(
        manifest.package.build,
        BuildScript(Some("build.rs".to_owned()))
    );
    assert!(manifest.lib.proc_macro);
    let support = &manifest.dependencies[&DependencyKind::Normal]["support"];
    assert_eq!(
        support.source,
        RequestSource::Path {
            path: "../support".to_owned()
        }
    );
    Ok(())
}

#[test]
fn writes_back_the_stub_manifest_it_reads() -> Result<(), serde_json::Error> {
    let manifest: StubManifest = serde_json::from_value(stored_manifest())?;
    assert_eq!(serde_json::to_value(&manifest)?, stored_manifest());
    Ok(())
}

#[test]
fn refuses_a_dependency_with_default_features() {
    let mut stored = stored_manifest();
    stored["dependencies"]["support"]["default-features"] = json!(true);
    assert!(serde_json::from_value::<StubManifest>(stored).is_err());
}

#[test]
fn writes_a_missing_build_script_as_false() -> Result<(), serde_json::Error> {
    let mut stored = stored_manifest();
    stored["package"]["build"] = json!(false);
    let manifest: StubManifest = serde_json::from_value(stored.clone())?;
    assert_eq!(manifest.package.build, BuildScript(None));
    assert_eq!(serde_json::to_value(&manifest)?, stored);
    Ok(())
}

#[test]
fn reads_stored_feature_activations() -> Result<(), serde_json::Error> {
    let stored = json!({
        "build-dependencies": {
            "syn": { "default-features": false, "features": ["full"], "version": "=3.0.6" }
        }
    });
    let activations: FeatureActivations = serde_json::from_value(stored.clone())?;
    assert_eq!(
        activations[&DependencyKind::Build]["syn"].version.0,
        "3.0.6"
    );
    assert_eq!(serde_json::to_value(&activations)?, stored);
    Ok(())
}
