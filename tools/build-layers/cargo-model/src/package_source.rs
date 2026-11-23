// SPDX-License-Identifier: GPL-3.0-only
use crate::cargo_text::{CargoText, ReadByCargo};
use cargo_util_schemas::core::SourceKind;
use cargo_util_schemas::lockfile::TomlLockfileSourceId;

impl ReadByCargo for TomlLockfileSourceId {
    fn read(text: &str) -> Option<Self> {
        Self::new(text.to_owned()).ok()
    }
}

pub type PackageSource = CargoText<TomlLockfileSourceId>;

impl PackageSource {
    #[must_use]
    pub fn is_external(&self) -> bool {
        !matches!(self, Self::Read(source) if *source.kind() == SourceKind::Path)
    }
}
