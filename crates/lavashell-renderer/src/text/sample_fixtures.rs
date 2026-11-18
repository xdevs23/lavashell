// SPDX-License-Identifier: GPL-3.0-only
use crate::test_surfaces::TestSurface;

pub const PLAIN_LABEL_TEXT: &str = "Mon 5 Oct office";

pub const ACCENTED_LABEL_TEXT: &str = "Mon 5 Oct e\u{301} office";

const HEBREW_WORD: &str = "אבג";

pub const LABEL_SURFACE: TestSurface = TestSurface::new(300, 48);

#[must_use]
pub fn mixed_script_label_text() -> String {
    format!("{ACCENTED_LABEL_TEXT} {HEBREW_WORD}")
}

#[must_use]
pub fn multiline_label_text() -> String {
    format!("{ACCENTED_LABEL_TEXT}\nabc {HEBREW_WORD}")
}
