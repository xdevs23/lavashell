// SPDX-License-Identifier: GPL-3.0-only
use super::{DebugInfo, Strip, StripLevel, TargetKind, Unit, UnitIdentity, UnitMode};

const CARGO_UNIT: &str = r#"{
    "pkg_id": "registry+https://github.com/rust-lang/crates.io-index#thiserror@2.0.21",
    "target": {
        "kind": ["lib"], "crate_types": ["lib"], "name": "thiserror",
        "src_path": "/cargo/registry/src/thiserror-2.0.21/src/lib.rs",
        "edition": "2021", "doc": true, "doctest": true, "test": true
    },
    "profile": {
        "name": "release", "opt_level": "3", "lto": "false", "codegen_backend": null,
        "codegen_units": null, "debuginfo": 0, "split_debuginfo": null,
        "debug_assertions": false, "overflow_checks": false, "rpath": false,
        "incremental": false, "panic": "unwind", "strip": {"resolved": {"Named": "debuginfo"}}
    },
    "platform": "x86_64-unknown-linux-gnu",
    "mode": "build",
    "features": ["default", "std"],
    "dependencies": [
        {"index": 36, "extern_crate_name": "build_script_build", "public": false,
         "noprelude": false, "nounused": false},
        {"index": 37, "extern_crate_name": "thiserror_impl"}
    ]
}"#;

#[test]
fn reads_a_unit_written_by_cargo() -> Result<(), serde_json::Error> {
    let unit: Unit = serde_json::from_str(CARGO_UNIT)?;
    assert!(unit.pkg_id.is_external());
    assert_eq!(unit.target.kind, [TargetKind::Lib]);
    assert_eq!(unit.mode, UnitMode::Build);
    assert_eq!(unit.profile.debuginfo, Some(DebugInfo::Level(0)));
    assert_eq!(
        unit.profile.strip,
        Strip::Resolved(StripLevel::Named("debuginfo".to_owned()))
    );
    assert!(unit.dependencies.iter().all(|edge| !edge.public));
    Ok(())
}

#[test]
fn reads_back_the_identity_it_writes() -> Result<(), serde_json::Error> {
    let identity = serde_json::from_str::<Unit>(CARGO_UNIT)?.identity();
    let written = serde_json::to_string(&identity)?;
    assert_eq!(serde_json::from_str::<UnitIdentity>(&written)?, identity);
    Ok(())
}
