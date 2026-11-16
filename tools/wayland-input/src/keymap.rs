// SPDX-License-Identifier: GPL-3.0-only
use crate::error::SessionError;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use xkbcommon::xkb;

const CONFIG_ROOT: &str = env!(
    "LAVASHELL_XKB_CONFIG_ROOT",
    "LAVASHELL_XKB_CONFIG_ROOT is not set. Build inside `nix develop`, \
     or set it to the X11/xkb directory of xkeyboard-config."
);

// https://gitlab.freedesktop.org/wayland/wayland/-/blob/main/protocol/wayland.xml
const PROTOCOL_KEYCODE_OFFSET: u32 = 8;
// https://gitlab.freedesktop.org/wayland/wayland/-/blob/main/protocol/wayland.xml
const TEXT_TERMINATOR: u8 = 0;

pub struct Keymap(xkb::Keymap);

impl Keymap {
    pub fn compile() -> Result<Self, SessionError> {
        let mut context =
            xkb::Context::new(xkb::CONTEXT_NO_DEFAULT_INCLUDES | xkb::CONTEXT_NO_ENVIRONMENT_NAMES);
        if !context.include_path_append(Path::new(CONFIG_ROOT)) {
            return Err(SessionError::KeymapData(CONFIG_ROOT));
        }
        let (rules, model, layout, variant, options) = ("", "", "", "", None);
        let flags = xkb::KEYMAP_COMPILE_NO_FLAGS;
        xkb::Keymap::new_from_names(&context, rules, model, layout, variant, options, flags)
            .map(Self)
            .ok_or(SessionError::Keymap(CONFIG_ROOT))
    }

    pub fn shared_file(&self) -> Result<(File, u32), SessionError> {
        let runtime_directory =
            std::env::var_os("XDG_RUNTIME_DIR").ok_or(SessionError::RuntimeDirectoryMissing)?;
        let mut text = self
            .0
            .get_as_string(xkb::KEYMAP_FORMAT_TEXT_V1)
            .into_bytes();
        text.push(TEXT_TERMINATOR);
        let size = u32::try_from(text.len()).map_err(SessionError::KeymapTooLarge)?;
        let mut file =
            tempfile::tempfile_in(runtime_directory).map_err(SessionError::CreateKeymapFile)?;
        file.write_all(&text).map_err(SessionError::WriteKeymap)?;
        Ok((file, size))
    }

    pub fn code(&self, name: &str) -> Result<u32, SessionError> {
        (!name.contains('\0'))
            .then(|| self.0.key_by_name(name))
            .flatten()
            .and_then(|key| key.raw().checked_sub(PROTOCOL_KEYCODE_OFFSET))
            .ok_or_else(|| SessionError::UnknownKey(name.to_owned()))
    }
}

#[cfg(test)]
mod tests;
