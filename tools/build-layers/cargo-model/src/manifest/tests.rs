// SPDX-License-Identifier: GPL-3.0-only
use super::FeatureActivations;
use crate::dependency_kind::DependencyKind;
use serde_json::json;

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
