// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::accessibility::{AccessibleText, AccessibleTextRun, TextDirection};
use lavashell_core::prelude::Rect;
use std::collections::BTreeSet;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Unit {
    pub scalars: usize,
    pub bytes: u8,
}

#[derive(Clone, Debug)]
pub(super) struct NativeRun {
    pub start: usize,
    pub text: String,
    pub line: usize,
    pub direction: TextDirection,
    pub character_bounds: Vec<Rect>,
    pub units: Vec<Unit>,
    pub word_starts: Vec<u8>,
}

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
            (!part.chars().all(char::is_whitespace)).then_some(start)
        })
        .collect()
}

// https://docs.rs/accesskit/0.25.1/accesskit/struct.Node.html#method.word_starts
const ACCESSKIT_CHARACTERS_PER_RUN: u8 = u8::MAX;

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
    if units.is_empty() {
        return vec![native(0, 0, &[], Vec::new())];
    }
    let mut start = 0;
    units
        .chunks(usize::from(ACCESSKIT_CHARACTERS_PER_RUN))
        .map(|chunk| {
            let mut end = start;
            let mut word_starts = Vec::new();
            for (index, unit) in (0..ACCESSKIT_CHARACTERS_PER_RUN).zip(chunk) {
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

fn units(run: &AccessibleTextRun) -> Vec<Unit> {
    let sizes = run.text.chars().map(char::len_utf8).collect::<Vec<_>>();
    selectable_units(&run.selectable_lengths, &sizes)
        .or_else(|| selectable_units(&vec![1; sizes.len()], &sizes))
        .unwrap_or_default()
}

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

#[cfg(test)]
mod tests;
