// SPDX-License-Identifier: GPL-3.0-only
use super::{SystemImage, prepare_device};
use crate::error::RoundTripError;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

#[derive(Debug, thiserror::Error)]
enum TestError {
    #[error(transparent)]
    RoundTrip(#[from] RoundTripError),
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("the tool prepared a device although the system images do not fit the ABI")]
    DevicePrepared,
}

const ABI: &str = "x86_64";
const GPU_MODE: &str = "swiftshader_indirect";

fn emulator_sdk(images: &[&str]) -> Result<TempDir, io::Error> {
    let sdk = tempfile::tempdir()?;
    for image in images {
        fs::create_dir_all(sdk.path().join("system-images").join(image))?;
    }
    Ok(sdk)
}

fn system_image(sdk: &Path) -> SystemImage<'_> {
    SystemImage {
        emulator_sdk: sdk,
        abi: ABI,
    }
}

#[test]
fn writes_a_device_that_uses_the_system_image_of_the_sdk() -> Result<(), TestError> {
    let work = tempfile::tempdir()?;
    let sdk = emulator_sdk(&[
        "platform-36/default/x86_64",
        "platform-35/default/arm64-v8a",
    ])?;
    let environment = prepare_device(work.path(), &system_image(sdk.path()), GPU_MODE)?;
    let device = work.path().join("avd/probe.avd");
    assert_eq!(
        fs::read_to_string(work.path().join("avd/probe.ini"))?,
        format!("path={}\ntarget=platform-36\n", device.display())
    );
    assert_eq!(
        fs::read_to_string(device.join("config.ini"))?,
        "abi.type=x86_64\nhw.cpu.arch=x86_64\nimage.sysdir.1=system-images/platform-36/default/x86_64/\n\
         tag.id=default\nhw.gpu.enabled=yes\nhw.gpu.mode=swiftshader_indirect\n"
    );
    assert!(work.path().join("home").is_dir());
    let mut command = Command::new("adb");
    let configured = environment
        .with("ANDROID_ADB_SERVER_PORT", "5037")
        .apply(&mut command);
    let variables: Vec<_> = configured
        .get_envs()
        .filter_map(|(name, value)| Some((name.to_str()?, value?.to_str()?)))
        .collect();
    assert!(variables.contains(&(
        "ANDROID_AVD_HOME",
        &*work.path().join("avd").to_string_lossy()
    )));
    assert!(variables.contains(&("TMPDIR", &*work.path().to_string_lossy())));
    assert!(variables.contains(&("ANDROID_ADB_SERVER_PORT", "5037")));
    Ok(())
}

#[test]
fn a_device_needs_exactly_one_system_image_for_its_abi() -> Result<(), TestError> {
    let work = tempfile::tempdir()?;
    for images in [
        &["platform-36/default/arm64-v8a"][..],
        &["platform-35/default/x86_64", "platform-36/default/x86_64"],
    ] {
        let sdk = emulator_sdk(images)?;
        let found = match prepare_device(work.path(), &system_image(sdk.path()), GPU_MODE) {
            Err(RoundTripError::SystemImageNotUnique { platforms, .. }) => platforms,
            Ok(_environment) => return Err(TestError::DevicePrepared),
            Err(error) => return Err(error.into()),
        };
        let expected: Vec<OsString> = images
            .iter()
            .filter(|image| image.ends_with(ABI))
            .filter_map(|image| image.split('/').next())
            .map(OsString::from)
            .collect();
        assert_eq!(found, expected);
    }
    Ok(())
}
