// SPDX-License-Identifier: GPL-3.0-only
use super::Shaper;
use crate::failure::DrawFailure;
use crate::paint::{dp_of, rect_of};
use lavashell_core::accessibility::TextDirection as Direction;
use lavashell_core::prelude::*;
use lavashell_core::view::{TextWidths, min_text_width};
use std::collections::HashMap;

impl TextMeasurer for Shaper {
    fn observe_rendering(&mut self) {
        self.text_configuration.track();
    }

    fn measure(&mut self, text: &str, style: &TextStyle<Dp>, max_width: Dp) -> Size {
        let size = (self.prepare_paragraph(text, style, max_width))
            .map(|paragraph| paragraph_size(paragraph, max_width));
        self.measured(size).unwrap_or_default()
    }

    fn min_width(&mut self, text: &str, style: &TextStyle<Dp>) -> Dp {
        min_text_width(text, style, |text| {
            let text_widths = (self.prepare_paragraph(text, style, Dp::INFINITY))
                .map(|paragraph| widths(paragraph));
            self.measured(text_widths).unwrap_or(NO_WIDTHS)
        })
    }

    fn geometry(&mut self, text: &str, style: &TextStyle<Dp>, width: Dp) -> TextGeometry {
        let geometry = (self.prepare_paragraph("", style, Dp::INFINITY))
            .map(|empty| dp_of(empty.height()))
            .and_then(|empty_line| {
                let paragraph = self.prepare_paragraph(text, style, width)?;
                Ok(shape(paragraph, empty_line, text, width, true))
            });
        self.measured(geometry).unwrap_or_default()
    }

    fn private_measure(
        &mut self,
        text: &lavashell_core::protected_text::PrivateText,
        style: &TextStyle<Dp>,
        max_width: Dp,
    ) -> Size {
        let rendering = self.text_configuration.get().rendering;
        let size = text.expose(|value| {
            (self.private_paragraph(rendering, value, style, max_width))
                .map(|paragraph| paragraph_size(&paragraph, max_width))
        });
        self.measured(size).unwrap_or_default()
    }

    fn private_min_width(
        &mut self,
        text: &lavashell_core::protected_text::PrivateText,
        style: &TextStyle<Dp>,
    ) -> Dp {
        let rendering = self.text_configuration.get().rendering;
        text.expose(|value| {
            min_text_width(value, style, |text| {
                let text_widths = (self.private_paragraph(rendering, text, style, Dp::INFINITY))
                    .map(|paragraph| widths(&paragraph));
                self.measured(text_widths).unwrap_or(NO_WIDTHS)
            })
        })
    }

    fn private_geometry(
        &mut self,
        text: &lavashell_core::protected_text::PrivateText,
        style: &TextStyle<Dp>,
        width: Dp,
    ) -> TextGeometry {
        let rendering = self.text_configuration.get().rendering;
        let empty = self.private_paragraph(rendering, "", style, Dp::INFINITY);
        let Some(empty_line) = self.measured(empty.map(|empty| dp_of(empty.height()))) else {
            return TextGeometry::default();
        };
        text.geometry(|value| {
            let geometry = (self.private_paragraph(rendering, value, style, width))
                .map(|mut paragraph| shape(&mut paragraph, empty_line, value, width, false));
            self.measured(geometry).unwrap_or_default()
        })
    }
}

/// The widths of a text that is left unmeasured.
const NO_WIDTHS: TextWidths = TextWidths {
    widest_line: Dp::ZERO,
    longest_word: Dp::ZERO,
};

impl Shaper {
    /// The measure of a text. When the numbers of the text lie beyond the range of Skia scalars, records the failure
    /// and returns None. The caller then uses an empty measure.
    fn measured<T>(&self, measure: Result<T, DrawFailure>) -> Option<T> {
        measure
            .inspect_err(|failure| self.failures.record(*failure))
            .ok()
    }
}

fn widths(paragraph: &skia_safe::textlayout::Paragraph) -> TextWidths {
    TextWidths {
        longest_word: dp_of(paragraph.min_intrinsic_width().ceil()),
        widest_line: dp_of(paragraph.max_intrinsic_width().ceil()),
    }
}

fn paragraph_size(paragraph: &skia_safe::textlayout::Paragraph, max_width: Dp) -> Size {
    Size::new(
        dp_of(paragraph.max_intrinsic_width().ceil()).min(max_width),
        dp_of(paragraph.height().ceil()),
    )
}

/// The carets, clusters, lines and, when `words` is set, word starts of `text` laid out in `paragraph`. `empty_line`
/// is the height of a line without text in the style of `paragraph`.
fn shape(
    paragraph: &mut skia_safe::textlayout::Paragraph,
    empty_line: Dp,
    text: &str,
    width: Dp,
    words: bool,
) -> TextGeometry {
    use skia_safe::textlayout::TextDirection;
    let mut carets = Vec::new();
    let mut clusters: Vec<TextCluster> = Vec::new();
    let mut utf16 = 0;
    let units = text.chars().map(char::len_utf16).sum();
    let utf16_bytes = (text.char_indices())
        .scan(0, |unit, (byte, ch)| {
            let start = *unit;
            *unit += ch.len_utf16();
            Some((start, byte))
        })
        .chain([(units, text.len())])
        .collect::<HashMap<_, _>>();
    let lines = line_ranges(paragraph, &utf16_bytes);
    let word_starts = words
        .then(|| word_starts(paragraph, text, &utf16_bytes, units))
        .flatten();
    let mut line = 0;
    let mut last = Rect::new(Dp::ZERO, Dp::ZERO, Dp::ZERO, empty_line);
    for (index, ch) in text.char_indices() {
        if ch == '\n' {
            // ICU groups CR and LF into one selectable cluster. The cluster
            // before the LF then already contains it.
            if !clusters
                .last()
                .is_some_and(|cluster| cluster.byte_range.contains(&index))
            {
                carets.push(TextCaret {
                    index,
                    rect: last,
                    line,
                });
                clusters.push(TextCluster {
                    byte_range: index..index + 1,
                    bounds: last,
                    direction: clusters
                        .last()
                        .map_or(Direction::Auto, |cluster| cluster.direction),
                    line,
                });
            }
            line += 1;
            last = Rect::new(Dp::ZERO, last.y + last.height, Dp::ZERO, empty_line);
        } else if let Some(info) = paragraph.get_glyph_info_at_utf16_offset(utf16)
            && info.grapheme_cluster_text_range.start == utf16
        {
            let r = info.grapheme_layout_bounds;
            let rtl = info.text_direction == TextDirection::RTL;
            let glyph_line = paragraph
                .get_line_number_at_utf16_offset(utf16)
                .unwrap_or(line);
            let x = if rtl { r.right } else { r.left };
            let height = if r.height() > 0.0 {
                dp_of(r.height())
            } else {
                empty_line
            };
            let rect = Rect::new(dp_of(x), dp_of(r.top), Dp::ZERO, height);
            if let Some(end) = utf16_bytes.get(&info.grapheme_cluster_text_range.end) {
                clusters.push(TextCluster {
                    byte_range: index..*end,
                    bounds: Rect {
                        height,
                        ..rect_of(r)
                    },
                    direction: if rtl {
                        Direction::RightToLeft
                    } else {
                        Direction::LeftToRight
                    },
                    line: glyph_line,
                });
            }
            if glyph_line > line {
                carets.push(TextCaret {
                    index,
                    rect: last,
                    line,
                });
                line = glyph_line;
            }
            carets.push(TextCaret { index, rect, line });
            let end = if rtl { r.left } else { r.right };
            last = Rect::new(dp_of(end), rect.y, Dp::ZERO, rect.height);
        }
        utf16 += ch.len_utf16();
    }
    carets.push(TextCaret {
        index: text.len(),
        rect: last,
        line,
    });
    TextGeometry {
        carets,
        size: Size::new(width, dp_of(paragraph.height()).max(last.y + last.height)),
        clusters,
        lines,
        word_starts,
    }
}

/// The byte range of every line of `paragraph`.
fn line_ranges(
    paragraph: &skia_safe::textlayout::Paragraph,
    utf16_bytes: &HashMap<usize, usize>,
) -> Vec<std::ops::Range<usize>> {
    let mut previous_line_end = 0;
    paragraph
        .get_line_metrics()
        .iter()
        .filter_map(|line| {
            let start = *utf16_bytes.get(&line.start_index)?;
            let end = *utf16_bytes.get(&line.end_including_newline)?;
            // Skia reports the empty last line with the range of the LF before it.
            // The line starts after that LF, so its range is empty.
            let start = start.max(previous_line_end);
            previous_line_end = end;
            Some(start..end)
        })
        .collect()
}

fn word_starts(
    paragraph: &skia_safe::textlayout::Paragraph,
    text: &str,
    utf16_bytes: &HashMap<usize, usize>,
    length: usize,
) -> Option<Vec<usize>> {
    let mut starts = Vec::new();
    let mut offset = 0;
    while offset < length {
        // Skia and ICU report word boundaries in UTF-16 units. The geometry returned to the core uses UTF-8 offsets.
        let range = paragraph.get_word_boundary(u32::try_from(offset).ok()?);
        if range.end <= offset || range.end > length {
            return None;
        }
        let start = *utf16_bytes.get(&range.start)?;
        let end = *utf16_bytes.get(&range.end)?;
        if text[start..end].chars().any(|ch| !ch.is_whitespace()) {
            starts.push(start);
        }
        offset = range.end;
    }
    Some(starts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::failure::Failures;
    use crate::test_error::TestError;

    fn style() -> TextStyle<Dp> {
        TextStyle {
            max_lines: Some(1),
            ..TextStyle::new("sans-serif", 14.sp(), Color::WHITE)
        }
        .resolve_unscaled()
    }

    #[test]
    fn text_sized_beyond_the_range_of_skia_scalars_measures_as_empty() -> Result<(), TestError> {
        let mut shaper = Shaper::new(&crate::Fonts::new(), Failures::default())?;
        let style =
            TextStyle::new("sans-serif", Sp::new(f64::MAX), Color::WHITE).resolve_unscaled();
        assert_eq!(shaper.measure("Display", &style, Dp::INFINITY), Size::ZERO);
        assert_eq!(shaper.min_width("Display", &style), Dp::ZERO);
        assert!(
            shaper
                .geometry("Display", &style, 100.dp())
                .carets
                .is_empty()
        );
        Ok(())
    }

    #[test]
    fn wrapping_text_is_at_least_as_wide_as_its_longest_word() -> Result<(), TestError> {
        let mut shaper = Shaper::new(&crate::Fonts::new(), Failures::default())?;
        for max_lines in [Some(2), None] {
            let style = TextStyle {
                max_lines,
                ..style()
            };
            let longest = shaper.measure("brightness", &style, Dp::INFINITY).width;
            assert!(longest > Dp::ZERO);
            assert_eq!(shaper.min_width("Display brightness", &style), longest);
        }
        Ok(())
    }

    #[test]
    fn one_line_text_is_as_wide_as_its_ellipsis_or_itself_if_narrower() -> Result<(), TestError> {
        let mut shaper = Shaper::new(&crate::Fonts::new(), Failures::default())?;
        let mut width = |text: &str| shaper.measure(text, &style(), Dp::INFINITY).width;
        let (ellipsis, word, dot) = (width("…"), width("Display"), width("."));
        assert!(Dp::ZERO < dot && dot < ellipsis && ellipsis < word);
        assert_eq!(shaper.min_width("Display brightness", &style()), ellipsis);
        assert_eq!(shaper.min_width(".", &style()), dot);
        assert_eq!(shaper.min_width("", &style()), Dp::ZERO);
        Ok(())
    }

    #[test]
    fn a_probe_at_zero_width_measures_text_at_its_smallest_width() -> Result<(), TestError> {
        let mut shaper = Shaper::new(&crate::Fonts::new(), Failures::default())?;
        let zero = Proposal {
            width: Some(0.dp()),
            height: None,
        };
        let mut wrapping = View::new(lavashell_test_platform::facts(), |ui| {
            ui.text("Display brightness", Modifier::new().max_lines(2));
        });
        let two_lines = TextStyle {
            max_lines: Some(2),
            ..style()
        };
        let longest = shaper.measure("brightness", &two_lines, Dp::INFINITY);
        let probed = wrapping.size_that_fits(zero, &mut shaper)?;
        assert_eq!(probed.width, longest.width);
        assert!(probed.height > longest.height);
        let mut one_line = View::new(lavashell_test_platform::facts(), |ui| {
            ui.text("Display brightness", Modifier::new().max_lines(1));
        });
        assert_eq!(
            one_line.size_that_fits(zero, &mut shaper)?,
            shaper.measure("…", &style(), Dp::INFINITY)
        );
        Ok(())
    }

    #[test]
    fn skia_clusters_use_real_bidi_bounds_and_utf8_ranges() -> Result<(), TestError> {
        let mut shaper = Shaper::new(&crate::Fonts::new(), Failures::default())?;
        let value = "abcאבג🙂";
        let geometry = shaper.geometry(value, &style(), 500.dp());
        assert!(!geometry.clusters.is_empty());
        let reconstructed = geometry
            .clusters
            .iter()
            .map(|cluster| {
                assert!(value.is_char_boundary(cluster.byte_range.start));
                assert!(value.is_char_boundary(cluster.byte_range.end));
                assert!(cluster.bounds.x.is_finite() && cluster.bounds.width >= 0.dp());
                &value[cluster.byte_range.clone()]
            })
            .collect::<String>();
        assert_eq!(reconstructed, value);
        assert!(
            geometry
                .clusters
                .iter()
                .any(|cluster| cluster.direction == Direction::RightToLeft)
        );
        let narrow = style().size * 1.5;
        assert!(
            geometry.clusters.iter().any(|cluster| {
                &value[cluster.byte_range.clone()] == "c" && cluster.bounds.width < narrow
            }),
            "an LTR glyph must not span its adjoining RTL run"
        );
        Ok(())
    }

    #[test]
    fn skia_combining_and_zwj_clusters_match_keyboard_caret_boundaries() -> Result<(), TestError> {
        let mut shaper = Shaper::new(&crate::Fonts::new(), Failures::default())?;
        let value = "e\u{301}👩\u{200d}💻z";
        let geometry = shaper.geometry(value, &style(), 500.dp());
        assert!(!geometry.carets.iter().any(|caret| caret.index == 1));
        for cluster in &geometry.clusters {
            assert!(
                geometry
                    .carets
                    .iter()
                    .any(|caret| caret.index == cluster.byte_range.start)
            );
            assert!(
                geometry
                    .carets
                    .iter()
                    .any(|caret| caret.index == cluster.byte_range.end)
            );
        }
        let first = geometry.clusters.first();
        assert_eq!(first.map(|cluster| cluster.byte_range.clone()), Some(0..3));
        let last = geometry.clusters.last();
        assert_eq!(
            last.map(|cluster| cluster.byte_range.end),
            Some(value.len())
        );
        Ok(())
    }

    #[test]
    fn skia_icu_word_starts_convert_surrogates_and_skip_whitespace() -> Result<(), TestError> {
        let mut shaper = Shaper::new(&crate::Fonts::new(), Failures::default())?;
        let value = "  café 🦀 κόσμος  \nשלום world";
        let geometry = shaper.geometry(value, &style(), 500.dp());
        let words = ["café", "🦀", "κόσμος", "שלום", "world"];
        let expected = words
            .iter()
            .map(|word| value.find(word))
            .collect::<Option<Vec<_>>>();
        assert!(expected.is_some());
        assert_eq!(geometry.word_starts, expected);
        Ok(())
    }

    #[test]
    fn skia_lines_convert_utf16_ranges_and_preserve_final_blank_line() -> Result<(), TestError> {
        let mut shaper = Shaper::new(&crate::Fonts::new(), Failures::default())?;
        let style = TextStyle {
            max_lines: None,
            ..style()
        };
        let value = "é🦀\nשלום\n";
        let geometry = shaper.geometry(value, &style, 500.dp());
        assert_eq!(geometry.lines, [0..7, 7..16, 16..16]);

        let value = "é🦀 alpha beta gamma";
        let geometry = shaper.geometry(value, &style, 70.dp());
        assert!(geometry.lines.len() > 1);
        let mut previous = 0;
        for range in geometry.lines {
            assert_eq!(range.start, previous);
            assert!(value.is_char_boundary(range.start) && value.is_char_boundary(range.end));
            previous = range.end;
        }
        assert_eq!(previous, value.len());
        Ok(())
    }

    #[test]
    fn skia_crlf_geometry_preserves_each_scalar_once() -> Result<(), TestError> {
        let mut shaper = Shaper::new(&crate::Fonts::new(), Failures::default())?;
        let style = TextStyle {
            max_lines: None,
            ..style()
        };
        let value = "é\r\nabc\r\n";
        let geometry = shaper.geometry(value, &style, 500.dp());
        assert_eq!(geometry.lines, [0..4, 4..9, 9..9]);
        assert!(
            !geometry
                .carets
                .iter()
                .any(|caret| [3, 8].contains(&caret.index))
        );
        assert_eq!(
            geometry
                .clusters
                .iter()
                .map(|cluster| &value[cluster.byte_range.clone()])
                .collect::<String>(),
            value
        );
        Ok(())
    }
}
