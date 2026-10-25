// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::test_error::{Required, TestResult};

#[test]
fn replacement_preserves_unicode() {
    let field = ProtectedText::new();
    assert_eq!(field.replace(0..0, "é"), 1);
    assert_eq!(field.replace(1..1, "ab"), 2);
    assert_eq!(field.masked('•'), "•••");
    assert_eq!(field.replace(0..1, "中x"), 2);
    assert_eq!(field.replace(0..4, "z"), 1);
    let private = field.take();
    private.expose(|value| assert_eq!(value, "z"));
    assert!(field.masked('•').is_empty());
}

#[test]
fn an_edit_in_the_middle_moves_the_rest_of_the_text() {
    let field = ProtectedText::new();
    assert_eq!(field.replace(0..0, "ac"), 2);
    assert_eq!(field.replace(1..1, "é💡"), 2);
    assert_eq!(field.replace(3..4, ""), 0);
    let private = field.take();
    private.expose(|value| assert_eq!(value, "aé💡"));
}

#[test]
fn line_breaks_become_spaces() {
    let field = ProtectedText::new();
    assert_eq!(field.replace(0..0, "a\r\nb"), 4);
    field.take().expose(|value| assert_eq!(value, "a  b"));
}

#[test]
fn reveal_payload_is_private_redacted_and_invalidated_by_edits_or_hiding() -> TestResult {
    let field = ProtectedText::new();
    assert_eq!(field.replace(0..0, "private-é💡"), 10);
    assert!(field.presentation('•').is_none());
    field.set_revealed(true);
    let first = field.presentation('•').required("presentation")?;
    assert_eq!(format!("{first:?}"), "PrivateText([redacted])");
    assert_eq!(first, field.presentation('•').required("presentation")?);
    assert_eq!(field.replace(0..1, "x"), 1);
    let second = field.presentation('•').required("presentation")?;
    assert_ne!(first, second);
    field.set_revealed(false);
    assert!(field.presentation('•').is_none());
    assert!(field.presentation.borrow().is_none());
    first.expose(|value| assert_eq!(value, "private-é💡"));
    second.expose(|value| assert_eq!(value, "xrivate-é💡"));
    assert_eq!(field.masked('•'), "••••••••••");
    Ok(())
}

#[test]
fn the_reveal_payload_follows_the_mask_it_is_asked_for() -> TestResult {
    let field = ProtectedText::new();
    assert_eq!(field.replace(0..0, "é💡"), 2);
    field.set_revealed(true);
    let dotted = field.presentation('•').required("presentation")?;
    let starred = field.presentation('*').required("presentation")?;
    assert_ne!(dotted, starred);
    assert_eq!(dotted.masked(), "••");
    assert_eq!(starred.masked(), "**");
    assert_eq!(starred, field.presentation('*').required("presentation")?);
    Ok(())
}
