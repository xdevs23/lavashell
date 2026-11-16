// SPDX-License-Identifier: GPL-3.0-only
use crate::test_error::TestError;
use std::ffi::OsString;

const TEST_FONT: &str = "LAVASHELL_TEST_FONT";
const STATIC_TEST_FONTS: &str = "LAVASHELL_TEST_STATIC_FONTS";

pub(crate) fn test_font() -> Result<crate::Fonts, TestError> {
    Ok(crate::Fonts::new().file("probe", required_variable(TEST_FONT)?)?)
}

pub(crate) fn static_test_faces() -> Result<crate::Fonts, TestError> {
    let paths = required_variable(STATIC_TEST_FONTS)?;
    Ok(crate::Fonts::new().files("probe", std::env::split_paths(&paths))?)
}

fn required_variable(name: &'static str) -> Result<OsString, TestError> {
    std::env::var_os(name).ok_or(TestError::UnsetVariable(name))
}
