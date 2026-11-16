// SPDX-License-Identifier: GPL-3.0-only
use crate::test_surfaces::TestSurface;

pub(crate) const PLAIN_LABEL_TEXT: &str = "Mon 5 Oct office";

pub(crate) const ACCENTED_LABEL_TEXT: &str = "Mon 5 Oct e\u{301} office";

const HEBREW_WORD: &str = "אבג";

pub(crate) const LABEL_SURFACE: TestSurface = TestSurface::new(300, 48);

pub(crate) fn mixed_script_label_text() -> String {
    format!("{ACCENTED_LABEL_TEXT} {HEBREW_WORD}")
}

pub(crate) fn multiline_label_text() -> String {
    format!("{ACCENTED_LABEL_TEXT}\nabc {HEBREW_WORD}")
}
