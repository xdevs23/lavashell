// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::clipboard::tests::{TestError, TestResult, move_bytes, receive_request, set_request};
use crate::{ActionQueue, ClipboardTransfers, SurfaceRecords, Watches};
use lavashell_core::prelude::{
    AccessibilityAction, ActionOutcome, ActionRequest, Density, Dp, DpExt, Modifier, NodeId, Point,
    PreferenceOverrides, Semantics, Size, State, TextFieldOptions, TextMeasurer, TextSelection,
    TextStyle, Viewport,
};
use lavashell_platform::{ClipboardEvent, FormatId, SelectionRefusal};
use lavashell_test_fixtures::{RecordedScene, WithoutFailures};
use std::io::Write;

struct Metrics;

impl TextMeasurer for Metrics {
    fn measure(&mut self, value: &str, _: &TextStyle<Dp>, width: Dp) -> Size {
        let natural: Dp = value.chars().map(|_| 10.dp()).sum();
        Size::new(natural.min(width), 20.dp())
    }
}

const WINDOW: Viewport = Viewport::new(Size::new(Dp::new(220.0), Dp::new(100.0)), Density::ONE);

/// A view that shows a text field named `field` with the text "abc def", after its first frame, and the state of the
/// text.
fn field_view(modifier: Modifier) -> Result<(View, State<String>), TestError> {
    let value = State::new("abc def".to_string());
    let shown = value.clone();
    let mut view = View::new(lavashell_test_fixtures::facts(), move |ui| {
        let shown = shown.clone();
        ui.text_area(
            shown.get(),
            TextFieldOptions::default(),
            modifier
                .clone()
                .accessible_name("field")
                .size(140.dp(), 60.dp()),
            move |text| shown.set(text),
        );
    });
    (view.frame_into(WINDOW, &mut Metrics, &mut RecordedScene::default())).without_failures()?;
    Ok((view, value))
}

/// A run with one surface, whose view shows a focused text field with the text "abc def" and "def" selected.
struct Editor {
    watches: Watches,
    actions: ActionQueue,
    transfers: ClipboardTransfers,
    text: TextClipboard,
    records: SurfaceRecords,
    surface: SurfaceId,
    value: State<String>,
}

impl Editor {
    fn new() -> Result<Self, TestError> {
        let (mut view, value) = field_view(Modifier::new())?;
        let field = field_id(&view)?;
        let selection = TextSelection {
            anchor: 4,
            focus: 7,
        };
        for action in [
            AccessibilityAction::Focus,
            AccessibilityAction::SetTextSelection(selection),
        ] {
            let outcome = view.accessibility_action(ActionRequest {
                target: field,
                action,
            });
            assert_eq!(outcome, Ok(ActionOutcome::Performed));
        }
        let mut records = SurfaceRecords::new(PreferenceOverrides::default());
        let surface = records.open(view, WINDOW.size, WINDOW.density, None)?;
        let watches = Watches::default();
        let actions = ActionQueue::new(&watches);
        let transfers = ClipboardTransfers::new(&watches);
        let text = TextClipboard::new(transfers.clipboard(), actions.view_actions());
        Ok(Self {
            watches,
            actions,
            transfers,
            text,
            records,
            surface,
            value,
        })
    }

    /// Applies Control with `key` to the view, and starts the clipboard requests that follow from it.
    fn press(&mut self, key: char) -> Result<bool, TestError> {
        let view =
            (self.records.open_view_mut(self.surface)).ok_or(TestError::Missing("the view"))?;
        let applied = self.text.apply_shortcut(self.surface, &control(key), view);
        self.transfers.submit(&self.records);
        Ok(applied)
    }

    fn run_actions(&mut self) -> TestResult {
        let view =
            (self.records.open_view_mut(self.surface)).ok_or(TestError::Missing("the view"))?;
        self.actions.take_batch().run(self.surface, view);
        Ok(())
    }
}

/// The id of the accessibility node named `field` in `view`.
fn field_id(view: &View) -> Result<NodeId, TestError> {
    (view.accessibility().nodes.values())
        .find(|node| node.name.as_deref() == Some("field"))
        .map(|node| node.id)
        .ok_or(TestError::Missing("the field"))
}

fn control(key: char) -> InputEvent {
    InputEvent::Shortcut(Shortcut::control(Key::Character(key)))
}

fn focus_field(view: &mut View) {
    view.input(InputEvent::Press(Point::new(11.dp(), 5.dp())));
    view.input(InputEvent::Release(Point::new(11.dp(), 5.dp())));
}

fn select_all(view: &mut View) {
    view.input(control('a'));
}

fn read_only(semantics: &mut Semantics) {
    semantics.state.form.read_only = true;
}

#[test]
fn cut_text_leaves_the_field_once_the_selection_is_set() -> TestResult {
    let mut editor = Editor::new()?;
    assert!(editor.press('x')?);
    let (id, _) = set_request(editor.transfers.take_requests())?;
    editor.run_actions()?;
    assert_eq!(editor.value.get(), "abc def");
    editor
        .transfers
        .handle(ClipboardEvent::SelectionSet(id, Ok(())));
    assert_eq!(editor.value.get(), "abc def");
    editor.run_actions()?;
    assert_eq!(editor.value.get(), "abc ");
    Ok(())
}

#[test]
fn cut_text_stays_in_the_field_when_the_selection_is_refused() -> TestResult {
    let mut editor = Editor::new()?;
    assert!(editor.press('x')?);
    let (id, _) = set_request(editor.transfers.take_requests())?;
    let refusal = Err(SelectionRefusal::NoInput);
    editor
        .transfers
        .handle(ClipboardEvent::SelectionSet(id, refusal));
    editor.run_actions()?;
    assert_eq!(editor.value.get(), "abc def");
    Ok(())
}

#[test]
fn a_cut_needs_selected_text_in_the_focused_field() -> TestResult {
    let cut = Key::Character('x');
    let (mut view, _) = field_view(Modifier::new())?;
    assert!(clipboard_shortcut(&mut view, cut).is_none());
    focus_field(&mut view);
    assert!(clipboard_shortcut(&mut view, cut).is_none());
    select_all(&mut view);
    assert!(matches!(
        clipboard_shortcut(&mut view, cut),
        Some(ClipboardShortcut::Cut(source)) if source.text() == "abc def"
    ));
    Ok(())
}

#[test]
fn a_read_only_field_copies_but_does_not_cut() -> TestResult {
    let (mut view, _) = field_view(Modifier::new().semantics_with(read_only))?;
    focus_field(&mut view);
    select_all(&mut view);
    let source = (view.text_selection_source(field_id(&view)?))
        .ok_or(TestError::Missing("the selected text"))?;
    assert_eq!(source.text(), "abc def");
    assert!(!source.can_move());
    assert!(clipboard_shortcut(&mut view, Key::Character('x')).is_none());
    assert!(matches!(
        clipboard_shortcut(&mut view, Key::Character('c')),
        Some(ClipboardShortcut::Copy(text)) if text == "abc def"
    ));
    Ok(())
}

#[test]
fn a_paste_needs_an_editable_focused_field() -> TestResult {
    let paste = Key::Character('v');
    let (mut view, _) = field_view(Modifier::new())?;
    assert!(clipboard_shortcut(&mut view, paste).is_none());
    focus_field(&mut view);
    assert!(matches!(
        clipboard_shortcut(&mut view, paste),
        Some(ClipboardShortcut::Paste)
    ));
    Ok(())
}

#[test]
fn paste_without_an_edited_field_is_not_a_clipboard_shortcut() {
    let watches = Watches::default();
    let actions = ActionQueue::new(&watches);
    let transfers = ClipboardTransfers::new(&watches);
    let text = TextClipboard::new(transfers.clipboard(), actions.view_actions());
    let mut view = View::new(lavashell_test_fixtures::facts(), |_| {});
    assert!(!text.apply_shortcut(SurfaceId(0), &control('v'), &mut view));
    transfers.submit(&SurfaceRecords::new(PreferenceOverrides::default()));
    assert!(transfers.take_requests().is_empty());
}

#[test]
fn pasted_text_lands_in_the_field_without_control_characters() -> TestResult {
    let mut editor = Editor::new()?;
    let formats = vec![FormatId("text/plain".into())];
    editor
        .transfers
        .handle(ClipboardEvent::SelectionOffered(formats));
    assert!(editor.press('v')?);
    let (id, _) = receive_request(editor.transfers.take_requests())?;
    let (reader, mut writer) = std::io::pipe()?;
    writer.write_all("x\u{7}y".as_bytes())?;
    drop(writer);
    editor
        .transfers
        .handle(ClipboardEvent::ReceiveStarted(id, Ok(reader.into())));
    move_bytes(&editor.watches);
    editor.run_actions()?;
    assert_eq!(editor.value.get(), "abc xy");
    Ok(())
}

#[test]
fn pasted_text_keeps_line_breaks_and_tabs_and_drops_other_control_characters() {
    assert_eq!(
        without_control_characters("a\u{7}b\tc\r\nd\u{1b}"),
        "ab\tc\r\nd"
    );
}
