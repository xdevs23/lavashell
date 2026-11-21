// SPDX-License-Identifier: GPL-3.0-only
use super::publish_staged_refresh;
use crate::error::Error;
use crate::files::{read_bytes, write_bytes};
use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

const STORED: &[u8] = b"{\"stored\": true}\n";
const REFRESHED: &[u8] = b"{\"refreshed\": true}\n";
const EDITED_DURING_REFRESH: &[u8] = b"{\"edited\": true}\n";
const NOT_UTF8: &[u8] = &[0xff, 0xfe, 0x00];

#[derive(Debug, thiserror::Error)]
enum TestError {
    #[error(transparent)]
    Refresh(#[from] Error),
    #[error(transparent)]
    Directory(#[from] std::io::Error),
}

fn checkout_projections() -> Result<TempDir, TestError> {
    let projections = TempDir::new()?;
    write_bytes(&projections.path().join("native.json"), STORED)?;
    write_bytes(&projections.path().join("taskbar.json"), STORED)?;
    write_bytes(&projections.path().join(".taskbar.json.swp"), NOT_UTF8)?;
    fs::create_dir(projections.path().join("notes"))?;
    Ok(projections)
}

fn names_in(directory: &Path) -> Result<BTreeSet<OsString>, std::io::Error> {
    fs::read_dir(directory)?
        .map(|entry| entry.map(|entry| entry.file_name()))
        .collect()
}

fn names(listed: &[&str]) -> BTreeSet<OsString> {
    listed.iter().map(OsString::from).collect()
}

#[test]
fn a_failed_refresh_leaves_every_projection_as_it_was() -> Result<(), TestError> {
    let projections = checkout_projections()?;
    let refreshed = publish_staged_refresh(projections.path(), |staged| {
        write_bytes(&staged.join("native.json"), REFRESHED)?;
        write_bytes(&staged.join("bevy.json"), REFRESHED)?;
        Err::<(), _>(Error::NoUnitsSelected)
    });
    assert!(matches!(refreshed, Err(Error::NoUnitsSelected)));
    assert_eq!(read_bytes(&projections.path().join("native.json"))?, STORED);
    assert_eq!(
        names_in(projections.path())?,
        names(&["native.json", "taskbar.json", ".taskbar.json.swp", "notes"])
    );
    Ok(())
}

#[test]
fn a_successful_refresh_publishes_the_projections_it_changed_and_added() -> Result<(), TestError> {
    let projections = checkout_projections()?;
    publish_staged_refresh(projections.path(), |staged| {
        write_bytes(&staged.join("native.json"), REFRESHED)?;
        write_bytes(&staged.join("bevy.json"), REFRESHED)
    })?;
    assert_eq!(
        read_bytes(&projections.path().join("native.json"))?,
        REFRESHED
    );
    assert_eq!(
        read_bytes(&projections.path().join("bevy.json"))?,
        REFRESHED
    );
    assert_eq!(
        names_in(projections.path())?,
        names(&[
            "native.json",
            "bevy.json",
            "taskbar.json",
            ".taskbar.json.swp",
            "notes"
        ])
    );
    Ok(())
}

#[test]
fn a_successful_refresh_keeps_an_edit_of_a_projection_it_left_alone() -> Result<(), TestError> {
    let projections = checkout_projections()?;
    let edited = projections.path().join("taskbar.json");
    publish_staged_refresh(projections.path(), |staged| {
        write_bytes(&edited, EDITED_DURING_REFRESH)?;
        write_bytes(&staged.join("native.json"), REFRESHED)
    })?;
    assert_eq!(read_bytes(&edited)?, EDITED_DURING_REFRESH);
    Ok(())
}

#[test]
fn the_stage_holds_every_file_of_the_checkout_byte_for_byte() -> Result<(), TestError> {
    let projections = checkout_projections()?;
    let staged_files = publish_staged_refresh(projections.path(), |staged| {
        Ok((
            names_in(staged).map_err(|source| Error::ListDirectory {
                path: staged.to_owned(),
                source,
            })?,
            read_bytes(&staged.join(".taskbar.json.swp"))?,
        ))
    })?;
    assert_eq!(
        staged_files,
        (
            names(&["native.json", "taskbar.json", ".taskbar.json.swp"]),
            NOT_UTF8.to_vec()
        )
    );
    Ok(())
}
