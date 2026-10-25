// SPDX-License-Identifier: GPL-3.0-only
//! The text of a protected field while the application reveals it. The `Debug` output shows no text. The copy of
//! the text is erased when it is dropped.

use super::{SecretText, mask_text};
use crate::text_edit::TextGeometry;
use std::{fmt, sync::Arc};

/// A copy of revealed protected text in a draw command, which never changes. It is never used as a scene string or
/// as a cache key. A renderer must shape and paint the text without storing the plain text in a cache.
#[derive(Clone)]
pub struct PrivateText {
    secret: Arc<SecretText>,
    /// The character the editor shows for each character of the secret.
    pub(super) mask: char,
}

impl PrivateText {
    pub(super) fn snapshot(secret: &SecretText, mask: char) -> Self {
        Self {
            secret: Arc::new(secret.copy()),
            mask,
        }
    }

    /// Lends the plain text to `read` for shaping or painting without a cache. The caller must not log or store the
    /// plain text.
    pub fn expose<R>(&self, read: impl FnOnce(&str) -> R) -> R {
        self.secret.expose(read)
    }

    #[must_use]
    pub fn masked(&self) -> String {
        self.expose(|value| mask_text(value, self.mask))
    }

    /// Shapes the plain text with `shape`, then converts the byte positions in the result into the byte positions of
    /// the masked text used by the editor.
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
            // Word boundaries would show where the words of the password start. Word selection then works on the
            // masked text, also while the characters are revealed.
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
