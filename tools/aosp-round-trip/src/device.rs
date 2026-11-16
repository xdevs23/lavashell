// SPDX-License-Identifier: GPL-3.0-only
use crate::error::{FileOperation, RoundTripError};
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const AVD_NAME: &str = "probe";
const SYSTEM_IMAGE_TAG: &str = "default";

pub struct SystemImage<'settings> {
    pub emulator_sdk: &'settings Path,
    pub abi: &'settings str,
}

impl SystemImage<'_> {
    fn platform(&self) -> Result<OsString, RoundTripError> {
        let directory = self.emulator_sdk.join("system-images");
        let listing_failed = || RoundTripError::file(FileOperation::List, &directory);
        let mut platforms = fs::read_dir(&directory)
            .map_err(listing_failed())?
            .map(|entry| entry.map_err(listing_failed()))
            .filter(|entry| {
                entry.as_ref().map_or(true, |entry| {
                    entry.path().join(SYSTEM_IMAGE_TAG).join(self.abi).is_dir()
                })
            })
            .map(|entry| entry.map(|entry| entry.file_name()))
            .collect::<Result<Vec<_>, _>>()?;
        platforms.sort();
        <[OsString; 1]>::try_from(platforms)
            .map(|[platform]| platform)
            .map_err(|platforms| RoundTripError::SystemImageNotUnique {
                directory: directory.clone(),
                abi: self.abi.to_owned(),
                platforms,
            })
    }
}

#[derive(Clone)]
pub struct DeviceEnvironment(Vec<(&'static str, OsString)>);

impl DeviceEnvironment {
    #[must_use]
    pub fn with(mut self, name: &'static str, value: impl Into<OsString>) -> Self {
        self.0.push((name, value.into()));
        self
    }

    pub fn apply<'command>(&self, command: &'command mut Command) -> &'command mut Command {
        command.envs(self.0.iter().map(|(name, value)| (name, value)))
    }
}

struct DeviceConfiguration(Vec<(&'static str, OsString)>);

impl DeviceConfiguration {
    fn write(&self, path: &Path) -> Result<(), RoundTripError> {
        let mut contents = OsString::new();
        for (key, value) in &self.0 {
            contents.push(key);
            contents.push("=");
            contents.push(value);
            contents.push("\n");
        }
        fs::write(path, contents.as_encoded_bytes())
            .map_err(RoundTripError::file(FileOperation::Write, path))
    }
}

pub fn prepare_device(
    work: &Path,
    image: &SystemImage<'_>,
    gpu_mode: &str,
) -> Result<DeviceEnvironment, RoundTripError> {
    let avd = work.join("avd");
    let device = avd.join(format!("{AVD_NAME}.avd"));
    let home = work.join("home");
    for directory in [&device, &home] {
        fs::create_dir_all(directory).map_err(RoundTripError::file(
            FileOperation::CreateDirectory,
            directory,
        ))?;
    }
    let platform = image.platform()?;
    DeviceConfiguration(vec![
        ("path", device.clone().into_os_string()),
        ("target", platform.clone()),
    ])
    .write(&avd.join(format!("{AVD_NAME}.ini")))?;
    DeviceConfiguration(vec![
        ("abi.type", image.abi.into()),
        ("hw.cpu.arch", image.abi.into()),
        (
            "image.sysdir.1",
            PathBuf::from("system-images")
                .join(&platform)
                .join(SYSTEM_IMAGE_TAG)
                .join(image.abi)
                .join("")
                .into_os_string(),
        ),
        ("tag.id", SYSTEM_IMAGE_TAG.into()),
        ("hw.gpu.enabled", "yes".into()),
        ("hw.gpu.mode", gpu_mode.into()),
    ])
    .write(&device.join("config.ini"))?;
    let sdk: &OsStr = image.emulator_sdk.as_os_str();
    Ok(DeviceEnvironment(vec![
        ("ANDROID_SDK_ROOT", sdk.into()),
        ("ANDROID_HOME", sdk.into()),
        ("ANDROID_AVD_HOME", avd.into_os_string()),
        ("ANDROID_EMULATOR_HOME", home.clone().into_os_string()),
        ("ANDROID_USER_HOME", home.into_os_string()),
        ("TMPDIR", work.as_os_str().into()),
        ("ADB_USB", "0".into()),
        ("ADB_MDNS", "0".into()),
        ("ADB_EMU", "0".into()),
    ]))
}

#[cfg(test)]
mod tests;
