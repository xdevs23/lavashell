// SPDX-License-Identifier: GPL-3.0-only
use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt;
use std::path::PathBuf;

const ENVIRONMENT_SEPARATOR: u8 = 0;
const ASSIGNMENT: u8 = b'=';

fn proc_entry(pid: u32, entry: &str) -> PathBuf {
    ["/proc", &pid.to_string(), entry].iter().collect()
}

pub fn executable_of(pid: u32) -> std::io::Result<PathBuf> {
    std::fs::read_link(proc_entry(pid, "exe"))
}

pub fn environment_value_of(pid: u32, name: &str) -> std::io::Result<Option<OsString>> {
    let environment = std::fs::read(proc_entry(pid, "environ"))?;
    Ok(environment
        .split(|&byte| byte == ENVIRONMENT_SEPARATOR)
        .find_map(|entry| {
            entry
                .strip_prefix(name.as_bytes())?
                .strip_prefix(&[ASSIGNMENT])
        })
        .map(|value| OsString::from_vec(value.to_vec())))
}
