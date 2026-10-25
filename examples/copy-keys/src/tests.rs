// SPDX-License-Identifier: GPL-3.0-only
use super::{Error, keys, variant};

#[test]
fn a_snake_case_key_becomes_its_capitalized_words() -> Result<(), Error> {
    assert_eq!(variant("copy_file_uri")?, "CopyFileUri");
    assert_eq!(variant("step_2")?, "Step2");
    assert_eq!(variant("intro")?, "Intro");
    Ok(())
}

#[test]
fn a_key_that_is_not_snake_case_is_refused() {
    for key in [
        "",
        "Intro",
        "_intro",
        "intro_",
        "two__words",
        "2nd",
        "copy-file",
    ] {
        assert!(
            matches!(variant(key), Err(Error::KeyName { .. })),
            "{key:?} must be refused"
        );
    }
}

#[test]
fn two_keys_with_one_variant_name_are_refused() {
    let rows = [("step_2", "First"), ("step2", "Second")]
        .map(|(key, text)| (key.to_owned(), text.to_owned()));
    assert!(matches!(
        keys(&rows),
        Err(Error::DuplicateVariant { key }) if key == "step2"
    ));
}
