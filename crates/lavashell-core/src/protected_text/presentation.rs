// SPDX-License-Identifier: GPL-3.0-only

use super::{SecretText, mask_text};
use crate::text_edit::TextGeometry;
use std::{fmt, sync::Arc};

#[cfg(feature = "serde")]
mod encoding;

#[derive(Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PrivateText {
    #[cfg_attr(feature = "serde", serde(with = "encoding"))]
    secret: Arc<SecretText>,
    pub(super) mask: char,
}

impl PrivateText {
    pub(super) fn snapshot(secret: &SecretText, mask: char) -> Self {
        Self {
            secret: Arc::new(secret.copy()),
            mask,
        }
    }

    pub fn expose<R>(&self, read: impl FnOnce(&str) -> R) -> R {
        self.secret.expose(read)
    }

    #[must_use]
    pub fn masked(&self) -> String {
        self.expose(|value| mask_text(value, self.mask))
    }

    pub fn geometry(&self, shape: impl FnOnce(&str) -> TextGeometry) -> TextGeometry {
        self.expose(|value| {
            let mut geometry = shape(value);
            let boundaries: Vec<_> = value
                .char_indices()
                .map(|(index, _)| index)
                .chain(std::iter::once(value.len()))
                .collect();
            let mask_index = |index| {
                boundaries
                    .partition_point(|byte| *byte <= index)
                    .saturating_sub(1)
                    * self.mask.len_utf8()
            };
            for caret in &mut geometry.carets {
                caret.index = mask_index(caret.index);
            }
            for cluster in &mut geometry.clusters {
                cluster.byte_range =
                    mask_index(cluster.byte_range.start)..mask_index(cluster.byte_range.end);
            }
            for line in &mut geometry.lines {
                *line = mask_index(line.start)..mask_index(line.end);
            }
            geometry.word_starts = None;
            geometry
        })
    }
}

impl PartialEq for PrivateText {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.secret, &other.secret)
    }
}

impl fmt::Debug for PrivateText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PrivateText([redacted])")
    }
}
