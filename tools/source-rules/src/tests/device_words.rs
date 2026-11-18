// SPDX-License-Identifier: GPL-3.0-only
use super::{Failure, findings, safe};
use crate::device_words::DeviceWord;
use crate::findings::Rule;

const CORE_ROOT: &str = "crates/lavashell-core/src/lib.rs";
const CORE_MODULE: &str = "crates/lavashell-core/src/press.rs";
const PLATFORM_ROOT: &str = "crates/lavashell-platform/src/lib.rs";
const KEYBOARD_ROOT: &str = "crates/lavashell-keyboard/src/lib.rs";

fn device_word(name: &str, word: DeviceWord) -> Rule {
    Rule::DeviceWord {
        name: name.to_owned(),
        word,
    }
}

#[test]
fn a_click_or_a_tap_in_any_spelling_of_a_name_is_reported_with_its_line() -> Result<(), Failure> {
    let module = concat!(
        "pub fn on_click() {}\n",
        "pub struct DoubleClick;\n",
        "const TAP_SLOP: u8 = 0;\n",
        "fn taps() { let tapped = 0; }\n",
        "macro_rules! names { () => { fn clicks() {} } }\n",
    );
    let root = safe("mod press;\n");
    assert_eq!(
        findings(&[(CORE_ROOT, &root), (CORE_MODULE, module)])?,
        [
            (CORE_MODULE, 1, device_word("on_click", DeviceWord::Click)),
            (
                CORE_MODULE,
                2,
                device_word("DoubleClick", DeviceWord::Click)
            ),
            (CORE_MODULE, 3, device_word("TAP_SLOP", DeviceWord::Tap)),
            (CORE_MODULE, 4, device_word("taps", DeviceWord::Tap)),
            (CORE_MODULE, 4, device_word("tapped", DeviceWord::Tap)),
            (CORE_MODULE, 5, device_word("clicks", DeviceWord::Click)),
        ]
        .map(|(path, line, rule)| (path.to_owned(), Some(line), rule))
    );
    Ok(())
}

#[test]
fn the_platform_crate_is_checked_like_the_core_crate() -> Result<(), Failure> {
    let root = safe("pub fn tap_at() {}\n");
    assert_eq!(
        findings(&[(PLATFORM_ROOT, &root)])?,
        [(
            PLATFORM_ROOT.to_owned(),
            Some(2),
            device_word("tap_at", DeviceWord::Tap)
        )]
    );
    Ok(())
}

#[test]
fn words_that_only_begin_with_the_letters_of_a_device_word_are_not_reported() -> Result<(), Failure>
{
    let root = safe(concat!(
        "fn tab_order() {}\nstruct DataPath;\nconst TABS: u8 = 0;\n",
        "fn tape() {}\nfn taper() {}\nfn press_and_release_at() {}\n",
    ));
    assert_eq!(findings(&[(CORE_ROOT, &root)])?, []);
    Ok(())
}

#[test]
fn a_crate_other_than_the_core_and_platform_crates_is_not_checked() -> Result<(), Failure> {
    let root = safe("pub fn click() {}\n");
    assert_eq!(findings(&[(KEYBOARD_ROOT, &root)])?, []);
    Ok(())
}
