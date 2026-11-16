// SPDX-License-Identifier: GPL-3.0-only
use crate::test_error::TestError;
use lavashell_core::prelude::*;
use std::ffi::OsString;

const TEST_FONT: &str = "LAVASHELL_TEST_FONT";
const STATIC_TEST_FONTS: &str = "LAVASHELL_TEST_STATIC_FONTS";
pub(crate) const PROBE_FAMILY: &str = "probe";
const PROBE_SIZE: Sp = Sp::new(13.0);

pub(crate) fn test_font() -> Result<crate::Fonts, TestError> {
    Ok(crate::Fonts::new().file(PROBE_FAMILY, required_variable(TEST_FONT)?)?)
}

pub(crate) fn static_test_faces() -> Result<crate::Fonts, TestError> {
    let paths = required_variable(STATIC_TEST_FONTS)?;
    Ok(crate::Fonts::new().files(PROBE_FAMILY, std::env::split_paths(&paths))?)
}

pub(crate) fn probe_style_before_scaling() -> TextStyle {
    TextStyle::new(PROBE_FAMILY, PROBE_SIZE, Color::WHITE)
}

pub(crate) fn probe_style() -> TextStyle<Dp> {
    probe_style_at(PROBE_SIZE)
}

pub(crate) fn probe_style_at(size: Sp) -> TextStyle<Dp> {
    probe_style_before_scaling().size(size).resolve_unscaled()
}

pub(crate) fn probe_modifier() -> Modifier {
    Modifier::new()
        .font_family(PROBE_FAMILY)
        .font_size(PROBE_SIZE)
}

fn required_variable(name: &'static str) -> Result<OsString, TestError> {
    std::env::var_os(name).ok_or(TestError::UnsetVariable(name))
}
