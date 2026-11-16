// SPDX-License-Identifier: GPL-3.0-only
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

const EXECUTABLE_BITS: u32 = 0o111;

pub fn find_on_path(program: &str) -> Option<PathBuf> {
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|directory| directory.join(program))
        .find(|candidate| {
            candidate.metadata().is_ok_and(|metadata| {
                metadata.is_file() && metadata.permissions().mode() & EXECUTABLE_BITS != 0
            })
        })
}
