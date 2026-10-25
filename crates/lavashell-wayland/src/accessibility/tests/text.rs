// SPDX-License-Identifier: GPL-3.0-only
//! Tests that read the exported tree through `accesskit_consumer`, the library used by the AccessKit platform
//! adapters to navigate text.

use super::*;
use accesskit_consumer::Tree;
use lavashell_core::numbers::{count_f64, whole_count};
use lavashell_core::units::Dp;

fn exported(value: &str, runs: Vec<AccessibleTextRun>) -> Result<(Tree, ids::Ids), TestError> {
    let mut source = tree();
    node_mut(&mut source, NodeId(1 << 63))?.text = Some(AccessibleText {
        value: value.into(),
        runs,
        ..Default::default()
    });
    let mut ids = ids::Ids::default();
    let tree = Tree::new(mapping::tree(&source, &mut ids, ""), true);
    Ok((tree, ids))
}

fn native_field<'a>(
    tree: &'a Tree,
    ids: &mut ids::Ids,
) -> Result<accesskit_consumer::NodeRef<'a>, TestError> {
    let id = ids.node(NodeId(1 << 63));
    (tree.state())
        .node_by_tree_local_id(id, TreeId::ROOT)
        .ok_or(TestError::MissingNativeNode(id))
}

/// The text of the line `index` of `field`.
fn line(field: &accesskit_consumer::NodeRef<'_>, index: usize) -> Result<String, TestError> {
    (field.line_range_from_index(index))
        .map(|range| range.text())
        .ok_or(TestError::MissingLine(index))
}

#[test]
fn native_word_navigation_uses_boundaries_in_both_directions() -> Result<(), TestError> {
    let (tree, mut ids) = exported("one two three", Vec::new())?;
    let field = native_field(&tree, &mut ids)?;
    let second = field.document_start().forward_to_word_start();
    assert_eq!(second.to_global_usv_index(), 4);
    assert_eq!(second.forward_to_word_start().to_global_usv_index(), 8);
    assert_eq!(
        field
            .document_end()
            .backward_to_word_start()
            .to_global_usv_index(),
        8
    );
    assert_eq!(second.backward_to_word_start().to_global_usv_index(), 0);

    let (tree, mut ids) = exported("élan שלום", Vec::new())?;
    assert_eq!(
        native_field(&tree, &mut ids)?
            .document_start()
            .forward_to_word_start()
            .to_global_usv_index(),
        5
    );
    Ok(())
}

pub(super) fn run(
    start: usize,
    value: &str,
    direction: TextDirection,
    y: Dp,
) -> Result<AccessibleTextRun, TestError> {
    let characters = value.chars().count();
    Ok(AccessibleTextRun {
        start,
        text: value.into(),
        direction,
        line: whole_count(y / 20.dp())?,
        selectable_lengths: vec![1; characters],
        character_bounds: (0..characters)
            .map(|i| Ok(Rect::new(10.dp() * count_f64(i)?, y, 10.dp(), 20.dp())))
            .collect::<Result<_, TestError>>()?,
        ..Default::default()
    })
}

#[test]
fn bidi_fragments_on_one_visual_line_form_one_native_line() -> Result<(), TestError> {
    let runs = vec![
        run(0, "abc", TextDirection::LeftToRight, 0.dp())?,
        run(3, "אבג", TextDirection::RightToLeft, 0.dp())?,
    ];
    let (tree, mut ids) = exported("abcאבג", runs)?;
    let field = native_field(&tree, &mut ids)?;
    assert_eq!(line(&field, 0)?, "abcאבג");
    assert!(field.line_range_from_index(1).is_none());
    assert_eq!(
        field
            .document_start()
            .forward_to_line_end()
            .to_global_usv_index(),
        6
    );
    assert_eq!(
        field
            .document_end()
            .backward_to_line_start()
            .to_global_usv_index(),
        0
    );
    Ok(())
}

#[test]
fn native_line_navigation_preserves_wrapping_and_hard_breaks() -> Result<(), TestError> {
    let (tree, mut ids) = exported(
        "abcאבג next\nlast",
        vec![
            run(0, "abc", TextDirection::LeftToRight, 0.dp())?,
            run(3, "אבג ", TextDirection::RightToLeft, 0.dp())?,
            run(7, "next\n", TextDirection::LeftToRight, 20.dp())?,
            run(12, "last", TextDirection::LeftToRight, 40.dp())?,
        ],
    )?;
    let field = native_field(&tree, &mut ids)?;
    for (index, expected) in ["abcאבג ", "next\n", "last"].into_iter().enumerate() {
        assert_eq!(line(&field, index)?, expected);
    }
    assert_eq!(
        field
            .document_start()
            .forward_to_line_start()
            .to_global_usv_index(),
        7
    );
    Ok(())
}

#[test]
fn geometry_free_hard_breaks_keep_crlf_and_the_trailing_empty_line() -> Result<(), TestError> {
    let (tree, mut ids) = exported("first\r\nsecond\n", Vec::new())?;
    let field = native_field(&tree, &mut ids)?;
    for (index, expected) in ["first\r\n", "second\n", ""].into_iter().enumerate() {
        assert_eq!(line(&field, index)?, expected);
    }
    assert!(field.line_range_from_index(3).is_none());
    Ok(())
}

#[test]
fn empty_editable_text_keeps_a_native_caret_and_empty_line() -> Result<(), TestError> {
    let mut source = tree();
    node_mut(&mut source, NodeId(1 << 63))?.text = Some(AccessibleText {
        selection: Some(TextSelection {
            anchor: 0,
            focus: 0,
        }),
        ..Default::default()
    });
    let mut ids = ids::Ids::default();
    let native = mapping::tree(&source, &mut ids, "");
    let consumer = Tree::new(native, true);
    let field = native_field(&consumer, &mut ids)?;
    assert!(field.supports_text_ranges());
    assert_eq!(field.document_range().text(), "");
    assert_eq!(field.document_start().to_global_usv_index(), 0);
    assert_eq!(field.document_end().to_global_usv_index(), 0);
    assert_eq!(
        field
            .text_selection()
            .ok_or(TestError::MissingSelection)?
            .text(),
        ""
    );
    assert_eq!(line(&field, 0)?, "");
    assert!(field.line_range_from_index(1).is_none());
    let selection = *field
        .data()
        .text_selection()
        .ok_or(TestError::MissingSelection)?;
    let action = actions::request(
        request(
            Action::SetTextSelection,
            ids.node(NodeId(1 << 63)),
            Some(ActionData::SetTextSelection(selection)),
        ),
        &source,
        &ids,
    )
    .ok_or(TestError::RefusedAction)?;
    assert_eq!(
        action.action,
        AccessibilityAction::SetTextSelection(TextSelection {
            anchor: 0,
            focus: 0
        })
    );
    Ok(())
}

#[test]
fn native_long_runs_do_not_wrap_word_indices_or_invent_lines() -> Result<(), TestError> {
    let value = format!("{} two three", "a".repeat(260));
    let (tree, mut ids) = exported(&value, Vec::new())?;
    let field = native_field(&tree, &mut ids)?;
    let second = field.document_start().forward_to_word_start();
    assert_eq!(second.to_global_usv_index(), 261);
    assert_eq!(second.forward_to_word_start().to_global_usv_index(), 265);
    assert_eq!(second.backward_to_word_start().to_global_usv_index(), 0);
    assert_eq!(line(&field, 0)?, value);
    assert!(field.line_range_from_index(1).is_none());
    let position = field
        .text_position_from_global_usv_index(265)
        .ok_or(TestError::MissingPosition(265))?
        .to_raw();
    let mut owner = node(1 << 63, Role::TextInput, "");
    owner.text = Some(AccessibleText {
        value,
        ..Default::default()
    });
    assert_eq!(
        super::super::text::scalar_position(position, &owner, &ids),
        Some(265)
    );
    Ok(())
}

#[test]
fn explicit_word_boundaries_preserve_editor_units_and_continuations() -> Result<(), TestError> {
    let mut first = run(0, "ae\u{301}", TextDirection::LeftToRight, 0.dp())?;
    first.selectable_lengths = vec![1, 2];
    first.word_starts = Some(vec![0, 1]);
    let mut continuation = run(3, "z suffix", TextDirection::LeftToRight, 0.dp())?;
    continuation.word_starts = Some(vec![2]);
    let (tree, mut ids) = exported("ae\u{301}z suffix", vec![first, continuation])?;
    let field = native_field(&tree, &mut ids)?;
    let second = field.document_start().forward_to_word_start();
    assert_eq!(second.to_global_usv_index(), 1);
    assert_eq!(second.forward_to_word_start().to_global_usv_index(), 5);
    assert_eq!(
        field
            .document_end()
            .backward_to_word_start()
            .to_global_usv_index(),
        5
    );

    let mut continuation = run(3, "def", TextDirection::LeftToRight, 0.dp())?;
    continuation.word_starts = Some(Vec::new());
    let (tree, mut ids) = exported(
        "abcdef",
        vec![
            run(0, "abc", TextDirection::LeftToRight, 0.dp())?,
            continuation,
        ],
    )?;
    assert_eq!(
        native_field(&tree, &mut ids)?
            .document_start()
            .forward_to_word_start()
            .to_global_usv_index(),
        6
    );
    Ok(())
}

#[test]
fn a_run_with_a_character_beyond_the_range_of_accesskit_coordinates_has_no_character_coordinates()
-> Result<(), TestError> {
    let mut far = run(0, "ab", TextDirection::LeftToRight, 0.dp())?;
    far.character_bounds[1].x = Dp::new(f64::MAX / 2.0);
    let mut source = tree();
    node_mut(&mut source, NodeId(1 << 63))?.text = Some(AccessibleText {
        value: "ab".into(),
        runs: vec![far],
        ..Default::default()
    });
    let native = mapping::tree(&source, &mut ids::Ids::default(), "");
    let (_, run) = native_node(&native, NativeRole::TextRun)?;
    assert_eq!(run.character_positions(), None);
    assert_eq!(run.character_widths(), None);
    Ok(())
}
