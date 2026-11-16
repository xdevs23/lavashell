// SPDX-License-Identifier: GPL-3.0-only
use crate::test_error::TestError;

pub(crate) fn test_font() -> Result<crate::Fonts, TestError> {
    let path = std::env::var_os("LAVASHELL_TEST_FONT").ok_or(TestError::MissingTestFont)?;
    Ok(crate::Fonts::new().file("probe", path)?)
}
