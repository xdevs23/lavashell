// SPDX-License-Identifier: GPL-3.0-only
use super::RunLabel;

#[test]
fn a_label_of_ascii_letters_digits_and_separators_is_accepted() {
    assert!(matches!("run-2_b".parse::<RunLabel>(), Ok(label) if label.to_string() == "run-2_b"));
}

#[test]
fn an_empty_label_or_one_with_other_characters_is_refused() {
    for text in ["", "run 2", "run/2", "läuft"] {
        assert!(text.parse::<RunLabel>().is_err(), "{text:?} was accepted");
    }
}
