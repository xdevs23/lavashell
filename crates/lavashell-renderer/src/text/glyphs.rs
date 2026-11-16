// SPDX-License-Identifier: GPL-3.0-only
use skia_safe::textlayout::Paragraph;

// https://learn.microsoft.com/en-us/typography/opentype/spec/otff#tables-related-to-color-fonts
const OPENTYPE_COLOR_TABLES: [&[u8; 4]; 5] = [b"COLR", b"CPAL", b"CBDT", b"sbix", b"SVG "];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Glyphs {
    Coverage,
    Colored,
}

impl Glyphs {
    pub fn of(paragraph: &mut Paragraph) -> Self {
        let mut glyphs = Self::Coverage;
        paragraph.visit(|_, run| {
            if let Some(run) = run {
                let colored = run.font().typeface().read_table_tags().is_none_or(|tags| {
                    OPENTYPE_COLOR_TABLES
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
