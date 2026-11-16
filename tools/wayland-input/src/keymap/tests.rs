// SPDX-License-Identifier: GPL-3.0-only
use super::Keymap;
use crate::error::SessionError;

// https://github.com/torvalds/linux/blob/master/include/uapi/linux/input-event-codes.h
const KEY_SPACE: u32 = 57;

#[test]
fn the_space_bar_becomes_the_linux_code_of_the_space_key() -> Result<(), SessionError> {
    assert_eq!(Keymap::compile()?.code("SPCE")?, KEY_SPACE);
    Ok(())
}

#[test]
fn a_name_that_the_keymap_lacks_is_rejected() -> Result<(), SessionError> {
    let keymap = Keymap::compile()?;
    for name in ["", "NO KEY", "SP\0CE"] {
        assert!(
            matches!(keymap.code(name), Err(SessionError::UnknownKey(_))),
            "{name:?} was accepted"
        );
    }
    Ok(())
}
