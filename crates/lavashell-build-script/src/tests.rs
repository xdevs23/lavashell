// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;

#[test]
fn a_path_that_is_not_utf8_is_refused_as_not_watchable() {
    let path = Path::new(OsStr::from_bytes(b"/nix/store/\xff-icons"));
    assert!(matches!(
        rerun_if_changed(path),
        Err(BuildScriptError::PathNotWatchable(refused)) if refused == path
    ));
}

#[test]
fn a_path_with_a_line_break_is_refused_as_not_watchable() {
    let path = Path::new("/nix/store/icons\ncargo::rustc-cfg=injected");
    assert!(matches!(
        rerun_if_changed(path),
        Err(BuildScriptError::PathNotWatchable(refused)) if refused == path
    ));
}
