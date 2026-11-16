// SPDX-License-Identifier: GPL-3.0-only
use super::{PlanError, external_records, verify};
use lavashell_cargo_model::unit_graph::{UnitGraph, UnitIndexError};
use serde_json::{Value, json};

const ENGINE_ROOT: &str = "bevy";

#[derive(Debug, thiserror::Error)]
enum TestError {
    #[error(transparent)]
    Graph(#[from] serde_json::Error),
    #[error(transparent)]
    UnitIndex(#[from] UnitIndexError),
    #[error(transparent)]
    Plan(#[from] PlanError),
}

fn unit(package: &str, name: &str, features: &[&str], dependencies: &[usize]) -> Value {
    json!({
        "pkg_id": package,
        "target": { "kind": ["lib"], "crate_types": ["lib"], "name": name },
        "profile": {
            "name": "packaged", "opt_level": "3", "lto": "false", "codegen_backend": null,
            "codegen_units": null, "debuginfo": 0, "split_debuginfo": null,
            "debug_assertions": false, "overflow_checks": false, "rpath": false,
            "incremental": false, "panic": "unwind", "strip": { "resolved": { "Named": "debuginfo" } }
        },
        "platform": "x86_64-unknown-linux-gnu",
        "mode": "build",
        "features": features,
        "dependencies": dependencies.iter().map(|index| json!({
            "index": index, "extern_crate_name": "dependency", "public": false,
            "noprelude": false, "nounused": false
        })).collect::<Vec<_>>()
    })
}

fn registry(name: &str) -> String {
    format!("registry+https://github.com/rust-lang/crates.io-index#{name}@0.17.3")
}

fn graph(units: &[Value]) -> Result<UnitGraph, serde_json::Error> {
    serde_json::from_value(json!({ "version": 1, "units": units, "roots": [units.len() - 1] }))
}

fn application(app_features: &[&str]) -> Result<UnitGraph, serde_json::Error> {
    graph(&[
        unit(&registry("bevy"), "bevy", &[], &[1]),
        unit(&registry("bevy_app"), "bevy_app", app_features, &[]),
        unit(
            "path+file:///work/examples/counter#0.1.0",
            "counter",
            &[],
            &[0],
        ),
    ])
}

#[test]
fn accepts_the_plan_it_was_recorded_from() -> Result<(), TestError> {
    let consumer = application(&["std"])?;
    let engine = external_records(&consumer)?;
    assert_eq!(verify(&engine, &consumer, ENGINE_ROOT)?.engine_units, 2);
    Ok(())
}

#[test]
fn refuses_a_plan_whose_engine_units_changed() -> Result<(), TestError> {
    let engine = external_records(&application(&["std"])?)?;
    let consumer = application(&["std", "trace"])?;
    assert!(matches!(
        verify(&engine, &consumer, ENGINE_ROOT),
        Err(PlanError::EngineUnitsChanged { count: 2 })
    ));
    Ok(())
}

#[test]
fn refuses_a_plan_whose_engine_needs_an_uncached_unit() -> Result<(), TestError> {
    let consumer = graph(&[
        unit(&registry("bevy"), "bevy", &[], &[1]),
        unit(
            "path+file:///work/crates/lavashell-core#0.1.0",
            "lavashell_core",
            &[],
            &[],
        ),
        unit(
            "path+file:///work/examples/counter#0.1.0",
            "counter",
            &[],
            &[0],
        ),
    ])?;
    let engine = external_records(&consumer)?;
    assert!(matches!(
        verify(&engine, &consumer, ENGINE_ROOT),
        Err(PlanError::EngineUnitsUncached { count: 1 })
    ));
    Ok(())
}

#[test]
fn reports_an_edge_to_a_missing_unit() -> Result<(), TestError> {
    let consumer = graph(&[unit(&registry("bevy"), "bevy", &[], &[7])])?;
    assert!(matches!(external_records(&consumer), Err(error) if error.index == 7));
    Ok(())
}

#[test]
fn accepts_stored_dependencies_in_any_order() -> Result<(), TestError> {
    let consumer = graph(&[
        unit(&registry("bevy"), "bevy", &[], &[1, 2]),
        unit(&registry("bevy_app"), "bevy_app", &[], &[]),
        unit(&registry("bevy_ecs"), "bevy_ecs", &[], &[]),
        unit(
            "path+file:///work/examples/counter#0.1.0",
            "counter",
            &[],
            &[0],
        ),
    ])?;
    let mut stored = serde_json::to_value(external_records(&consumer)?)?;
    for record in stored.as_array_mut().into_iter().flatten() {
        if let Some(dependencies) = record["dependencies"].as_array_mut() {
            dependencies.reverse();
        }
    }
    let engine = serde_json::from_value(stored)?;
    assert_eq!(verify(&engine, &consumer, ENGINE_ROOT)?.engine_units, 3);
    Ok(())
}
