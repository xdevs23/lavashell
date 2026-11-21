// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use tempfile::TempDir;
use toml::{Table, Value};

#[derive(Debug, thiserror::Error)]
enum TestError {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Registration(#[from] RegistrationError),
    #[error("the scan accepts crates that owe a registration")]
    MissingRegistrationAccepted,
}

struct TestCrate {
    name: &'static str,
    dependencies: &'static [&'static str],
    registers: bool,
}

const TOP: &str = "top";

fn table<'a>(entries: impl IntoIterator<Item = (&'a str, Value)>) -> Table {
    (entries.into_iter())
        .map(|(key, value)| (key.to_owned(), value))
        .collect()
}

fn nested_table<'a>(entries: impl IntoIterator<Item = (&'a str, Value)>) -> Value {
    Value::Table(table(entries))
}

fn write_manifest(directory: &Path, manifest: &Table) -> io::Result<()> {
    fs::create_dir_all(directory.join("src"))?;
    fs::write(manifest_of(directory), manifest.to_string())
}

fn workspace_with(crates: &[TestCrate]) -> io::Result<TempDir> {
    let root = TempDir::new()?;
    let members = (crates.iter()).map(|member| {
        (
            member.name,
            nested_table([("path", Value::from(member.name))]),
        )
    });
    write_manifest(
        root.path(),
        &table([(
            "workspace",
            nested_table([("dependencies", nested_table(members))]),
        )]),
    )?;
    for member in crates {
        let directory = root.path().join(member.name);
        let dependencies = (member.dependencies.iter())
            .map(|&dependency| (dependency, nested_table([("workspace", Value::from(true))])));
        write_manifest(
            &directory,
            &table([("dependencies", nested_table(dependencies))]),
        )?;
        if member.registers {
            fs::write(directory.join("src").join("register.rs"), "")?;
        }
    }
    Ok(root)
}

fn find_in(root: &TempDir) -> Result<Registrations, RegistrationError> {
    Registrations::find(&root.path().join(TOP))
}

#[test]
fn a_listed_crate_that_registers_gets_a_register_call() -> Result<(), TestError> {
    let root = workspace_with(&[
        TestCrate {
            name: TOP,
            dependencies: &["settings", "plain"],
            registers: false,
        },
        TestCrate {
            name: "settings",
            dependencies: &[],
            registers: true,
        },
        TestCrate {
            name: "plain",
            dependencies: &[],
            registers: false,
        },
    ])?;
    let registrations = find_in(&root)?;
    assert_eq!(registrations.registering_crates, ["settings"]);
    assert!((registrations.register_all()).contains("settings :: register :: register (store) ;"));
    Ok(())
}

#[test]
fn every_listed_crate_that_uses_a_registering_crate_without_registering_is_named()
-> Result<(), TestError> {
    let root = workspace_with(&[
        TestCrate {
            name: TOP,
            dependencies: &["settings", "first-source", "second-source"],
            registers: false,
        },
        TestCrate {
            name: "settings",
            dependencies: &[],
            registers: true,
        },
        TestCrate {
            name: "first-source",
            dependencies: &["settings"],
            registers: false,
        },
        TestCrate {
            name: "second-source",
            dependencies: &["settings"],
            registers: false,
        },
    ])?;
    let Err(RegistrationError::RegistrationMissing { missing }) = find_in(&root) else {
        return Err(TestError::MissingRegistrationAccepted);
    };
    let named: Vec<(&str, &str)> = (missing.iter())
        .map(|missing| (missing.dependent.as_str(), missing.extended.as_str()))
        .collect();
    assert_eq!(
        named,
        [("first-source", "settings"), ("second-source", "settings")]
    );
    Ok(())
}

#[test]
fn a_registering_crate_whose_registering_parent_is_not_listed_fails_naming_both()
-> Result<(), TestError> {
    let root = workspace_with(&[
        TestCrate {
            name: TOP,
            dependencies: &["source"],
            registers: false,
        },
        TestCrate {
            name: "settings",
            dependencies: &[],
            registers: true,
        },
        TestCrate {
            name: "source",
            dependencies: &["settings"],
            registers: true,
        },
    ])?;
    assert!(matches!(
        find_in(&root),
        Err(RegistrationError::ParentNotListed { child, parent, .. })
            if child == "source" && parent == "settings"
    ));
    Ok(())
}

#[test]
fn a_dependency_taken_from_the_workspace_without_an_entry_there_fails() -> Result<(), TestError> {
    let root = workspace_with(&[TestCrate {
        name: TOP,
        dependencies: &["unlisted"],
        registers: false,
    }])?;
    assert!(matches!(
        find_in(&root),
        Err(RegistrationError::WorkspaceEntryMissing { dependency, .. }) if dependency == "unlisted"
    ));
    Ok(())
}
