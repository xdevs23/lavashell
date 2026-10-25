// SPDX-License-Identifier: GPL-3.0-only
use skia_safe::textlayout::Paragraph;

/// The tags of the OpenType tables that give glyphs their own colors or bitmaps: COLR, CPAL, CBDT, sbix and SVG, from
/// the list of tables related to color fonts:
/// <https://learn.microsoft.com/en-us/typography/opentype/spec/otff#tables-related-to-color-fonts>
const COLOR_TABLES: [&[u8; 4]; 5] = [b"COLR", b"CPAL", b"CBDT", b"sbix", b"SVG "];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Glyphs {
    /// Every glyph is a coverage mask that the text color fills.
    Coverage,
    /// A font used by the paragraph has one of the color tables, or the table tags of that font cannot be read.
    Colored,
}

impl Glyphs {
    pub fn of(paragraph: &mut Paragraph) -> Self {
        let mut glyphs = Self::Coverage;
        paragraph.visit(|_, run| {
            if let Some(run) = run {
                let colored = run.font().typeface().read_table_tags().is_none_or(|tags| {
                    COLOR_TABLES
                        .iter()
                        .any(|tag| tags.contains(&u32::from_be_bytes(**tag)))
                });
                if colored {
                    glyphs = Self::Colored;
                }
            }
        });
        glyphs
    }
}
