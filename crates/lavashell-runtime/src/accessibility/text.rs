// SPDX-License-Identifier: GPL-3.0-only
mod runs;

use super::{ids::Ids, mapping};
use crate::single_precision;
use accesskit::{
    Node, NodeId, Role, TextDirection as NativeDirection, TextPosition, TextSelection,
};
use lavashell_core::accessibility::{AccessibleNode, TextDirection};
use lavashell_core::prelude::{Dp, Rect};
use runs::{NativeRun, Unit, prepare};

pub(super) fn apply(
    source: &AccessibleNode,
    node: &mut Node,
    ids: &mut Ids,
    nodes: &mut Vec<(NodeId, Node)>,
) {
    let Some(text) = &source.text else { return };
    node.set_value(text.value.clone());
    if let Some(placeholder) = &text.placeholder {
        node.set_placeholder(placeholder.clone());
    }
    if let Some(language) = &text.language {
        node.set_language(language.clone());
    }
    // A password input gets no text runs, so assistive technologies cannot move through its characters.
    if node.role() == Role::PasswordInput {
        return;
    }
    let runs = prepare(text);
    let mut children = node.children().to_vec();
    for (index, run) in runs.iter().enumerate() {
        let id = ids.run(source.id, index);
        children.push(id);
        let mut native = Node::new(Role::TextRun);
        native.set_value(run.text.clone());
        if index > 0 && runs[index - 1].line == run.line {
            native.set_previous_on_line(ids.run(source.id, index - 1));
        }
        if runs
            .get(index + 1)
            .is_some_and(|next| next.line == run.line)
        {
            native.set_next_on_line(ids.run(source.id, index + 1));
        }
        native.set_word_starts(run.word_starts.clone());
        native.set_character_lengths(run.units.iter().map(|unit| unit.bytes).collect::<Vec<_>>());
        let direction = match run.direction {
            TextDirection::RightToLeft => NativeDirection::RightToLeft,
            _ => NativeDirection::LeftToRight,
        };
        native.set_text_direction(direction);
        let character_bounds = unit_bounds(run);
        if let Some(bounds) = bounds(&character_bounds) {
            native.set_bounds(mapping::rect(bounds));
            let positions: Option<Vec<_>> = character_bounds
                .iter()
                // The position of a character is the distance of its leading edge from the leading edge of the run.
                .map(|character| match direction {
                    NativeDirection::RightToLeft => {
                        bounds.x + bounds.width - character.x - character.width
                    }
                    _ => character.x - bounds.x,
                })
                .map(coordinate)
                .collect();
            let widths: Option<Vec<_>> = (character_bounds.iter())
                .map(|character| coordinate(character.width))
                .collect();
            // AccessKit stores character positions and widths as `f32`. A run with a value beyond the range of `f32`
            // gets neither.
            if let (Some(positions), Some(widths)) = (positions, widths) {
                native.set_character_positions(positions);
                native.set_character_widths(widths);
            }
        }
        nodes.push((id, native));
    }
    node.set_children(children);
    if let Some(selection) = text.selection
        && let Some(anchor) = position(source, &runs, selection.anchor, ids)
        && let Some(focus) = position(source, &runs, selection.focus, ids)
    {
        node.set_text_selection(TextSelection { anchor, focus });
    }
}

fn position(
    source: &AccessibleNode,
    runs: &[NativeRun],
    offset: usize,
    ids: &mut Ids,
) -> Option<TextPosition> {
    let (index, run) = (runs.iter().enumerate().rev())
        .find(|(_, run)| offset >= run.start)
        .or_else(|| runs.first().map(|run| (0, run)))?;
    Some(TextPosition {
        node: ids.run(source.id, index),
        character_index: {
            let scalar = offset
                .saturating_sub(run.start)
                .min(run.text.chars().count());
            let mut at = 0;
            (run.units.iter())
                .take_while(|unit| {
                    at += unit.scalars;
                    at <= scalar
                })
                .count()
        },
    })
}

/// Converts `length` to the nearest `f32`, the type of an AccessKit character coordinate, or returns `None` when
/// `length` lies beyond the range of `f32`.
fn coordinate(length: Dp) -> Option<f32> {
    single_precision::nearest(length.get())
}

fn bounds(rects: &[Rect]) -> Option<Rect> {
    rects.iter().copied().reduce(Rect::union)
}

pub(super) fn scalar_position(
    position: TextPosition,
    owner: &AccessibleNode,
    ids: &Ids,
) -> Option<usize> {
    let (node, run) = ids.owner(position.node)?;
    if node != owner.id {
        return None;
    }
    let run = prepare(owner.text.as_ref()?).get(run?).cloned()?;
    let units = run.units.get(..position.character_index)?;
    Some(run.start + units.iter().map(|unit| unit.scalars).sum::<usize>())
}

fn unit_bounds(run: &NativeRun) -> Vec<Rect> {
    if run.character_bounds.len() != run.text.chars().count() {
        return Vec::new();
    }
    let mut offset = 0;
    (run.units.iter())
        .filter_map(|&Unit { scalars, .. }| {
            let result = bounds(&run.character_bounds[offset..offset + scalars]);
            offset += scalars;
            result
        })
        .collect()
}
