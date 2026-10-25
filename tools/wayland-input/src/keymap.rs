// SPDX-License-Identifier: GPL-3.0-only
use crate::error::SessionError;
use std::path::Path;
use xkbcommon::xkb;

/// The xkeyboard-config directory, set at build time, from which the keymap is compiled. The keymap uses no other
/// data, so neither the environment nor the xkb files installed on the host change the keymap.
const CONFIG_ROOT: &str = env!(
    "LAVASHELL_XKB_CONFIG_ROOT",
    "LAVASHELL_XKB_CONFIG_ROOT is not set. Build inside `nix develop`, \
     or set it to the X11/xkb directory of xkeyboard-config."
);

/// `wl_keyboard` counts a key of an `xkb_v1` keymap as its xkb keycode minus 8.
const PROTOCOL_KEYCODE_OFFSET: u32 = 8;

pub struct Keymap(xkb::Keymap);

impl Keymap {
    /// Compiles the keymap for the default rules, model, layout, variant and options of libxkbcommon.
    pub fn compile() -> Result<Self, SessionError> {
        let mut context =
            xkb::Context::new(xkb::CONTEXT_NO_DEFAULT_INCLUDES | xkb::CONTEXT_NO_ENVIRONMENT_NAMES);
        if !context.include_path_append(Path::new(CONFIG_ROOT)) {
            return Err(SessionError::KeymapData(CONFIG_ROOT));
        }
        // An empty name selects the default that libxkbcommon was built with.
        let (rules, model, layout, variant, options) = ("", "", "", "", None);
        let flags = xkb::KEYMAP_COMPILE_NO_FLAGS;
        xkb::Keymap::new_from_names(&context, rules, model, layout, variant, options, flags)
            .map(Self)
            .ok_or(SessionError::Keymap(CONFIG_ROOT))
    }

    /// The keymap in the text format expected by the virtual keyboard protocol.
    pub fn text(&self) -> String {
        self.0.get_as_string(xkb::KEYMAP_FORMAT_TEXT_V1)
    }

    /// The key code expected by the virtual keyboard protocol for the key named `name` in the keymap, such as `SPCE`.
    pub fn code(&self, name: &str) -> Result<u32, SessionError> {
        // `key_by_name` panics on a name with a NUL byte.
        (!name.contains('\0'))
            .then(|| self.0.key_by_name(name))
            .flatten()
            .and_then(|key| key.raw().checked_sub(PROTOCOL_KEYCODE_OFFSET))
            .ok_or_else(|| SessionError::UnknownKey(name.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::{Keymap, PROTOCOL_KEYCODE_OFFSET};
    use crate::error::SessionError;
    use crate::test_error::TestError;
    use xkbcommon::xkb::Keycode;

    #[test]
    fn a_key_name_becomes_the_code_of_that_key_in_the_protocol() -> Result<(), TestError> {
        let keymap = Keymap::compile()?;
        let keycodes = keymap.0.min_keycode().raw()..=keymap.0.max_keycode().raw();
        let Some((keycode, name)) = keycodes
            .map(Keycode::new)
            .find_map(|keycode| Some((keycode, keymap.0.key_get_name(keycode)?)))
        else {
            return Err(TestError::NoNamedKey);
        };
        assert_eq!(keymap.code(name)? + PROTOCOL_KEYCODE_OFFSET, keycode.raw());
        Ok(())
    }

    #[test]
    fn a_name_that_the_keymap_lacks_is_rejected() -> Result<(), TestError> {
        let keymap = Keymap::compile()?;
        for name in ["", "NO KEY", "SP\0CE"] {
            assert!(
                matches!(keymap.code(name), Err(SessionError::UnknownKey(_))),
                "{name:?} was accepted"
            );
        }
        Ok(())
    }
}
