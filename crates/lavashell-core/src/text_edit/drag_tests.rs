// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::test_error::{Required, TestResult};
use crate::units::DpExt;

#[test]
fn bidi_selection_uses_glyph_bounds_and_drag_destination_uses_wrapped_row() -> TestResult {
    let mut value = "abcאבג xyz".to_string();
    let rows = [
        (0, 0.dp(), 0),
        (1, 10.dp(), 0),
        (2, 20.dp(), 0),
        (3, 60.dp(), 0),
        (5, 50.dp(), 0),
        (7, 40.dp(), 0),
        (9, 60.dp(), 0),
        (10, 0.dp(), 1),
        (11, 10.dp(), 1),
        (12, 20.dp(), 1),
        (13, 30.dp(), 1),
    ];
    let clusters = [
        (0..1, 0.dp(), 0),
        (1..2, 10.dp(), 0),
        (2..3, 20.dp(), 0),
        (3..5, 50.dp(), 0),
        (5..7, 40.dp(), 0),
        (7..9, 30.dp(), 0),
        (9..10, 60.dp(), 0),
        (10..11, 0.dp(), 1),
        (11..12, 10.dp(), 1),
        (12..13, 20.dp(), 1),
    ];
    let geometry = TextGeometry {
        carets: rows
            .into_iter()
            .map(|(index, x, line): (usize, Dp, u8)| TextCaret {
                index,
                rect: Rect::new(x, 20.dp() * f64::from(line), 1.dp(), 20.dp()),
                line: usize::from(line),
            })
            .collect(),
        clusters: clusters
            .into_iter()
            .map(|(range, x, line): (Range<usize>, Dp, u8)| TextCluster {
                direction: if (3..9).contains(&range.start) {
                    TextDirection::RightToLeft
                } else {
                    TextDirection::LeftToRight
                },
                byte_range: range,
                bounds: Rect::new(x, 20.dp() * f64::from(line), 10.dp(), 20.dp()),
                line: usize::from(line),
            })
            .collect(),
        lines: vec![0..10, 10..13],
        size: Size::new(70.dp(), 40.dp()),
        ..Default::default()
    };
    assert_eq!(
        geometry.selection(2..3),
        [Rect::new(20.dp(), 0.dp(), 10.dp(), 20.dp())]
    );
    assert_eq!(
        geometry.selection(3..9),
        [Rect::new(30.dp(), 0.dp(), 30.dp(), 20.dp())]
    );
    let mut edit = TextEdit::at_end(&value);
    edit.anchor = Some(3);
    edit.cursor = 9;
    edit.press(&value, &geometry, Point::new(45.dp(), 5.dp()), false);
    assert_eq!(edit.selection(), 3..9);
    edit.drag_to(&geometry, Point::new(31.dp(), 25.dp()), 4.dp());
    let drop_caret = edit.drop_caret().required("drop caret")?;
    assert_eq!(drop_caret.index, value.len());
    assert_eq!(drop_caret.rect.y, 20.dp());
    edit.release(&mut value, true, true);
    assert_eq!(value, "abc xyzאבג");
    assert_eq!(edit.selection(), 7..13);
    Ok(())
}

#[test]
fn shifted_press_extends_selection_without_starting_a_move() {
    let value = "abcd";
    let geometry = TextGeometry {
        carets: (0..=4_u8)
            .map(|index| TextCaret {
                index: usize::from(index),
                rect: Rect::new(10.dp() * f64::from(index), 0.dp(), 1.dp(), 20.dp()),
                line: 0,
            })
            .collect(),
        size: Size::new(40.dp(), 20.dp()),
        ..Default::default()
    };
    let mut edit = TextEdit::at_end(value);
    edit.anchor = Some(0);
    edit.cursor = 2;
    edit.press(value, &geometry, Point::new(11.dp(), 5.dp()), true);
    assert_eq!(edit.selection(), 0..1);
    edit.drag_to(&geometry, Point::new(31.dp(), 5.dp()), 4.dp());
    assert_eq!(edit.selection(), 0..3);
    assert!(edit.drop_caret().is_none());
}
