// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::accessibility::{AccessibleText, AccessibleTextRun, TextDirection};
use lavashell_core::prelude::Rect;
use std::collections::BTreeSet;
use unicode_segmentation::UnicodeSegmentation;

/// A unit of a run at which a selection can start or end, with the number of Unicode scalars that it spans and the
/// number of its UTF-8 bytes. AccessKit stores the byte count of each unit in a `u8`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Unit {
    pub scalars: usize,
    pub bytes: u8,
}

/// A text run in the form expected by an AccessKit `TextRun` node. AccessKit addresses the units of a run with `u8`
/// indices.
#[derive(Clone, Debug)]
pub(super) struct NativeRun {
    /// The Unicode scalar offset of the run in the text.
    pub start: usize,
    pub text: String,
    pub line: usize,
    pub direction: TextDirection,
    /// One rectangle for each Unicode scalar value, or empty when the source run has no rectangle for every scalar.
    pub character_bounds: Vec<Rect>,
    pub units: Vec<Unit>,
    /// The units at which a word starts, in ascending order.
    pub word_starts: Vec<u8>,
}

/// Converts `text` into runs for AccessKit, with one run for each line when the core supplies no runs. AccessKit
/// addresses the units of a run with `u8` indices, so a longer run is split into runs of at most `CHARACTERS_PER_RUN`
/// units. A split falls only between selectable units, and every piece has the line number of its source run.
pub(super) fn prepare(text: &AccessibleText) -> Vec<NativeRun> {
    let input = if text.runs.is_empty() {
        fallback(text)
    } else {
        text.runs.clone()
    };
    let words = input
        .iter()
        .any(|run| run.word_starts.is_none())
        .then(|| word_starts(&text.value));
    input
        .iter()
        .flat_map(|run| split(run, words.as_deref()))
        .collect()
}

fn fallback(text: &AccessibleText) -> Vec<AccessibleTextRun> {
    let mut start = 0;
    let mut runs = Vec::new();
    for (line, value) in text.value.split_inclusive('\n').enumerate() {
        let length = value.chars().count();
        let mut selectable_lengths = vec![1; length];
        if value.ends_with("\r\n") {
            selectable_lengths.truncate(length.saturating_sub(2));
            selectable_lengths.push(2);
        }
        runs.push(AccessibleTextRun {
            start,
            text: value.into(),
            line,
            selectable_lengths,
            direction: text.direction,
            ..Default::default()
        });
        start += length;
    }
    if runs.is_empty() || text.value.ends_with('\n') {
        runs.push(AccessibleTextRun {
            start,
            line: runs.len(),
            direction: text.direction,
            ..Default::default()
        });
    }
    runs
}

fn word_starts(value: &str) -> Vec<usize> {
    let mut scalar = 0;
    value
        .split_word_bounds()
        .filter_map(|part| {
            let start = scalar;
            scalar += part.chars().count();
            // Whitespace starts no word, so it belongs to the word before it. Whitespace at the start of a
            // paragraph still belongs to a word, because AccessKit treats the start of a paragraph as a word start.
            (!part.chars().all(char::is_whitespace)).then_some(start)
        })
        .collect()
}

/// The most characters in one run. AccessKit stores the word starts of a run as `u8` character indices. The AccessKit
/// consumer also converts the position after the last character of a run to `u8`, so that position must fit as well
/// (accesskit 0.25.1 and `accesskit_consumer` 0.39.1):
/// <https://docs.rs/accesskit/0.25.1/accesskit/struct.Node.html#method.word_starts>
const CHARACTERS_PER_RUN: u8 = u8::MAX;

fn split(run: &AccessibleTextRun, fallback_words: Option<&[usize]>) -> Vec<NativeRun> {
    let units = units(run);
    let bytes = run
        .text
        .char_indices()
        .map(|(byte, _)| byte)
        .chain(std::iter::once(run.text.len()))
        .collect::<Vec<_>>();
    let words: BTreeSet<usize> = match &run.word_starts {
        Some(words) => words.iter().copied().collect(),
        None => (fallback_words.unwrap_or_default().iter())
            .filter_map(|word| word.checked_sub(run.start))
            .collect(),
    };
    let native = |start: usize, end: usize, units: &[Unit], word_starts: Vec<u8>| NativeRun {
        start: run.start + start,
        text: run.text[bytes[start]..bytes[end]].into(),
        line: run.line,
        direction: run.direction,
        character_bounds: if run.character_bounds.len() == bytes.len() - 1 {
            run.character_bounds[start..end].to_vec()
        } else {
            Vec::new()
        },
        units: units.to_vec(),
        word_starts,
    };
    // Keep an empty final line addressable, even though it has no selectable units.
    if units.is_empty() {
        return vec![native(0, 0, &[], Vec::new())];
    }
    let mut start = 0;
    units
        .chunks(usize::from(CHARACTERS_PER_RUN))
        .map(|chunk| {
            let mut end = start;
            let mut word_starts = Vec::new();
            // A chunk has at most 255 units and the range has 256 indices, so the zip pairs every unit with an index.
            for (index, unit) in (0..=u8::MAX).zip(chunk) {
                if words.contains(&end) {
                    word_starts.push(index);
                }
                end += unit.scalars;
            }
            let result = native(start, end, chunk, word_starts);
            start = end;
            result
        })
        .collect()
}

/// Returns one unit for each selectable length of `run` when the UTF-8 size of every such unit fits in the `u8` used by
/// AccessKit. Otherwise returns one unit for each Unicode scalar, so that a cluster larger than 255 bytes can still be
/// read. The core rejects an edit that starts or ends inside a selectable unit.
fn units(run: &AccessibleTextRun) -> Vec<Unit> {
    let sizes = run.text.chars().map(char::len_utf8).collect::<Vec<_>>();
    selectable_units(&run.selectable_lengths, &sizes).unwrap_or_else(|| {
        (run.text.chars())
            .map(|character| Unit {
                scalars: 1,
                bytes: utf8_length(character),
            })
            .collect()
    })
}

/// Builds one unit for each length in `lengths`, where `sizes` contains the UTF-8 size of each scalar of the run.
/// Returns `None` when the lengths are empty, contain a zero or do not add up to the number of scalars, or when the
/// UTF-8 size of a unit does not fit in a `u8`.
fn selectable_units(lengths: &[usize], sizes: &[usize]) -> Option<Vec<Unit>> {
    if lengths.is_empty() || lengths.contains(&0) || lengths.iter().sum::<usize>() != sizes.len() {
        return None;
    }
    let mut offset = 0;
    (lengths.iter())
        .map(|&scalars| {
            let bytes = sizes[offset..offset + scalars].iter().sum::<usize>();
            offset += scalars;
            Some(Unit {
                scalars,
                bytes: u8::try_from(bytes).ok()?,
            })
        })
        .collect()
}

/// The UTF-8 length of `character`, which is one to four bytes.
fn utf8_length(character: char) -> u8 {
    match character.len_utf8() {
        1 => 1,
        2 => 2,
        3 => 3,
        _ => 4,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_unit_too_large_for_a_u8_falls_back_to_one_unit_per_scalar() {
        let cluster = "e\u{301}".repeat(100);
        let run = AccessibleTextRun {
            text: cluster.clone(),
            selectable_lengths: vec![200],
            ..Default::default()
        };
        let expected: Vec<_> = (cluster.chars())
            .map(|character| Unit {
                scalars: 1,
                bytes: utf8_length(character),
            })
            .collect();
        assert_eq!(units(&run), expected);
        let fitting = AccessibleTextRun {
            text: "e\u{301}".into(),
            selectable_lengths: vec![2],
            ..Default::default()
        };
        assert_eq!(
            units(&fitting),
            [Unit {
                scalars: 2,
                bytes: 3
            }]
        );
    }

    #[test]
    fn word_starts_are_indices_of_units_within_each_chunk() {
        let text = "ab cd".repeat(60);
        let run = AccessibleTextRun {
            text: text.clone(),
            selectable_lengths: vec![1; 300],
            word_starts: Some((0..300).step_by(5).collect()),
            ..Default::default()
        };
        let runs = split(&run, None);
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].units.len(), 255);
        assert_eq!(runs[1].start, 255);
        assert_eq!(runs[0].word_starts.len(), 51);
        assert_eq!(runs[1].word_starts, [0, 5, 10, 15, 20, 25, 30, 35, 40]);
    }
}
