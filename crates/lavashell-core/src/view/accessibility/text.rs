// SPDX-License-Identifier: GPL-3.0-only
use super::snapshot;
use crate::accessibility::{AccessibleTextRun, TextDirection};
use crate::geometry::{Point, Rect, Scale};
use crate::ui::{FieldContent, NodeKind};
use crate::units::Dp;
use crate::view::{TextGeometry, View};

impl View {
    pub(super) fn accessibility_text_runs(&self, index: usize) -> Vec<AccessibleTextRun> {
        let id = self.nodes[index].id;
        let Some(shaped) = self.editing.positions.get(&id) else {
            return Vec::new();
        };
        let Some(geometry) = self.accessibility.geometry.get(&id) else {
            return Vec::new();
        };
        let (NodeKind::Text(current, ..)
        | NodeKind::TextInput {
            content: FieldContent::Plain { value: current, .. },
            ..
        }) = &self.nodes[index].kind
        else {
            return Vec::new();
        };
        if current != &shaped.value {
            return Vec::new();
        }
        if shaped.scale.x <= 0.0 || shaped.scale.y <= 0.0 {
            return Vec::new();
        }
        let above_centered_text = if self.text_selectable(index) {
            Dp::ZERO
        } else {
            let free = geometry.content.height / shaped.scale.y - shaped.geometry.size.height;
            free.max(Dp::ZERO) / 2.0
        };
        let origin = Point::new(
            geometry.content.x - shaped.scroll.x * shaped.scale.x,
            geometry.content.y + (above_centered_text - shaped.scroll.y) * shaped.scale.y,
        );
        let placement = Placement {
            origin,
            scale: shaped.scale,
            device_pixel: self.device_pixel(),
        };
        shaped_runs(current, &shaped.geometry, placement)
    }
}

#[derive(Clone, Copy)]
struct Placement {
    origin: Point,
    scale: Scale,
    device_pixel: Dp,
}

fn shaped_runs(
    value: &str,
    geometry: &TextGeometry,
    placement: Placement,
) -> Vec<AccessibleTextRun> {
    let mut builder = RunBuilder {
        value,
        placement,
        runs: Vec::new(),
    };
    if geometry.clusters.is_empty() {
        builder.push_caret_spans(geometry);
    } else {
        for cluster in &geometry.clusters {
            builder.push(
                cluster.byte_range.clone(),
                cluster.bounds,
                cluster.direction,
                cluster.line,
            );
        }
    }
    builder.push_line_end(geometry);
    let mut runs = builder.runs;
    mark_word_starts(&mut runs, value, geometry);
    runs
}

fn mark_word_starts(runs: &mut [AccessibleTextRun], value: &str, geometry: &TextGeometry) {
    let words = geometry.word_starts.as_ref().and_then(|words| {
        words
            .iter()
            .map(|&byte| {
                value
                    .is_char_boundary(byte)
                    .then(|| snapshot::scalar_offset(value, byte))
            })
            .collect::<Option<Vec<_>>>()
    });
    for run in runs {
        run.word_starts = words.as_ref().map(|words| {
            let end = run.start + run.text.chars().count();
            let mut starts: Vec<_> = words
                .iter()
                .copied()
                .filter(|start| (run.start..end).contains(start))
                .map(|start| start - run.start)
                .collect();
            starts.sort_unstable();
            starts.dedup();
            starts
        });
    }
}

struct RunBuilder<'a> {
    value: &'a str,
    placement: Placement,
    runs: Vec<AccessibleTextRun>,
}

impl RunBuilder<'_> {
    fn push_caret_spans(&mut self, geometry: &TextGeometry) {
        for (a, b) in geometry.carets.iter().zip(geometry.carets.iter().skip(1)) {
            let same_row = a.line == b.line;
            let direction = if same_row && b.rect.x < a.rect.x {
                TextDirection::RightToLeft
            } else if same_row {
                TextDirection::LeftToRight
            } else {
                (self.runs.last()).map_or(TextDirection::Auto, |run| run.direction)
            };
            let left = if same_row {
                a.rect.x.min(b.rect.x)
            } else {
                a.rect.x
            };
            let width = if same_row {
                (b.rect.x - a.rect.x).abs()
            } else {
                a.rect.width
            };
            self.push(
                a.index..b.index,
                Rect::new(left, a.rect.y, width, a.rect.height),
                direction,
                a.line,
            );
        }
    }

    fn push_line_end(&mut self, geometry: &TextGeometry) {
        let value = self.value;
        let last_line = if let Some(run) = self.runs.last() {
            run.line
        } else {
            let run = AccessibleTextRun {
                text: value.to_string(),
                ..Default::default()
            };
            let line = run.line;
            self.runs.push(run);
            line
        };
        if value.ends_with('\n') {
            let line = geometry
                .lines
                .iter()
                .position(|range| range.start == value.len())
                .unwrap_or(last_line + 1);
            self.runs.push(AccessibleTextRun {
                start: value.chars().count(),
                line,
                ..Default::default()
            });
        }
    }

    fn push(
        &mut self,
        range: std::ops::Range<usize>,
        rect: Rect,
        direction: TextDirection,
        line: usize,
    ) {
        if range.start >= range.end
            || range.end > self.value.len()
            || !self.value.is_char_boundary(range.start)
            || !self.value.is_char_boundary(range.end)
        {
            return;
        }
        let Placement {
            origin,
            scale,
            device_pixel,
        } = self.placement;
        let bounds = Rect::new(
            origin.x + rect.x * scale.x,
            origin.y + rect.y * scale.y,
            (rect.width * scale.x).max(device_pixel),
            rect.height * scale.y,
        );
        let text = &self.value[range.clone()];
        let length = text.chars().count();
        let start = snapshot::scalar_offset(self.value, range.start);
        let mut run = match self.runs.pop() {
            Some(run)
                if run.start + run.text.chars().count() == start
                    && run.direction == direction
                    && run.line == line =>
            {
                run
            }
            previous => {
                self.runs.extend(previous);
                AccessibleTextRun {
                    start,
                    line,
                    direction,
                    ..Default::default()
                }
            }
        };
        run.text.push_str(text);
        run.selectable_lengths.push(length);
        run.character_bounds
            .extend(std::iter::repeat_n(bounds, length));
        self.runs.push(run);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_error::{Required, TestError, TestResult};
    use crate::text_edit::TextCluster;
    use crate::units::DpExt;
    use crate::view::test_text::repeated;

    fn geometry(
        value: &str,
        lines: Vec<std::ops::Range<usize>>,
        words: Vec<usize>,
    ) -> Result<TextGeometry, TestError> {
        let clusters = value
            .char_indices()
            .enumerate()
            .map(|(index, (byte, ch))| {
                Some(TextCluster {
                    byte_range: byte..byte + ch.len_utf8(),
                    bounds: Rect::new(
                        repeated(10.dp(), index),
                        repeated(1.dp(), index % 2),
                        10.dp(),
                        20.dp(),
                    ),
                    direction: TextDirection::LeftToRight,
                    line: lines.iter().position(|line| line.contains(&byte))?,
                })
            })
            .collect::<Option<Vec<_>>>()
            .required("a line for every character")?;
        Ok(TextGeometry {
            clusters,
            lines,
            word_starts: Some(words),
            ..Default::default()
        })
    }

    fn placement() -> Placement {
        Placement {
            origin: Point::default(),
            scale: Scale { x: 1.0, y: 1.0 },
            device_pixel: 1.dp(),
        }
    }

    #[test]
    fn exact_lines_and_words_convert_multibyte_offsets_without_font_height_splits() -> TestResult {
        let value = "é🦀 abc\nz";
        let shape = geometry(value, vec![0..7, 7..11, 11..12], vec![0, 7, 11])?;
        let runs = shaped_runs(value, &shape, placement());
        assert_eq!(runs.len(), 3);
        assert_eq!(
            runs.iter().map(|run| run.line).collect::<Vec<_>>(),
            [0, 1, 2]
        );
        assert_eq!(
            runs.iter().map(|run| run.start).collect::<Vec<_>>(),
            [0, 3, 7]
        );
        assert_eq!(runs[0].text, "é🦀 ");
        assert_eq!(runs[1].text, "abc\n");
        for run in &runs {
            assert_eq!(run.word_starts.as_deref(), Some(&[0][..]));
        }
        Ok(())
    }

    #[test]
    fn trailing_blank_line_is_addressable_and_has_no_invented_word() -> TestResult {
        let value = "é\n";
        let shape = geometry(value, vec![0..3, 3..3], vec![0])?;
        let runs = shaped_runs(value, &shape, placement());
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[1].line, 1);
        assert_eq!(runs[1].start, 2);
        assert_eq!(runs[1].text, "");
        assert_eq!(runs[1].word_starts.as_deref(), Some(&[][..]));
        Ok(())
    }
}
