// SPDX-License-Identifier: GPL-3.0-only
use crate::test_error::TestError;

/// The fonts used by the tests: the font file at the path set in the environment variable `LAVASHELL_TEST_FONT`,
/// registered as the family `probe`.
pub(crate) fn test_font() -> Result<crate::Fonts, TestError> {
    let path = std::env::var_os("LAVASHELL_TEST_FONT").ok_or(TestError::MissingTestFont)?;
    Ok(crate::Fonts::new().file("probe", path)?)
}
