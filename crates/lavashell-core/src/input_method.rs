// SPDX-License-Identifier: GPL-3.0-only
//! What an input method, such as an on-screen keyboard, needs to know about a text field, and the
//! edits it sends to the field.

use crate::keyboard::Key;
use crate::reactive::State;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

/// What the action key of an input method does in a field, and what Enter does in a single-line
/// field.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TextAction {
    Search,
    Done,
    Next,
    Go,
    Send,
}

/// The kind of text expected by a field. An input method can show a matching keyboard, such as digits for
/// `Number`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextPurpose {
    #[default]
    Text,
    Number,
    Phone,
    Email,
    Url,
}

/// The letters written as capitals automatically by an input method.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Capitalization {
    #[default]
    None,
    /// The first letter of each sentence.
    Sentences,
    /// The first letter of each word.
    Words,
    /// Every letter.
    Characters,
}

/// The settings that tell an input method how to present itself for a field: the action key, the kind of text and
/// the automatic capitals.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct TextHints {
    /// The action of the field, or None for a plain Enter key.
    pub action: Option<TextAction>,
    pub purpose: TextPurpose,
    pub capitalization: Capitalization,
}

/// The number of a text session. A new number starts whenever another field gains focus.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct SessionId(pub u64);

impl SessionId {
    #[must_use]
    pub fn next(self) -> Self {
        Self(self.0 + 1)
    }
}

/// The number of presses on the field of a session since the field gained focus.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct FieldPresses(pub u64);

/// The field being edited, as an input method sees it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextSession {
    pub id: SessionId,
    pub presses: FieldPresses,
    pub hints: TextHints,
    pub multiline: bool,
    pub text: FieldText,
}

/// What an input method may know of the text in the field being edited.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FieldText {
    /// A protected field, such as a password field. The input method does not see its text.
    Protected,
    /// A field in a host that does not report the text of the field.
    Unknown,
    /// The text before the cursor, or before the selection.
    BeforeCursor(Rc<str>),
}

/// An edit sent by an input method to the field being edited.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextCommand {
    /// Replaces the selection with the text.
    Commit(String),
    /// Deletes the selection, or the character before the cursor.
    DeleteBackward,
    Key(Key),
    /// Presses Enter with this action in place of the action of the field. The `on_key` handlers
    /// on the field and on the nodes above it receive Enter first. When none of them consumes it, `Next`
    /// moves focus to the next field and `Done` clears focus.
    Action(TextAction),
}

/// Sends commands to the focused field of the view in which this sender was composed, and returns the session
/// of that field. The view applies the commands in order once the input or the timer that sent
/// them is handled. A view drops the commands that arrive while no field accepts text.
#[derive(Clone)]
pub struct TextSender {
    commands: Rc<RefCell<VecDeque<TextCommand>>>,
    session: State<Option<TextSession>>,
}

impl TextSender {
    pub(crate) fn new() -> Self {
        Self {
            commands: Rc::default(),
            session: State::new(None),
        }
    }

    pub fn send(&self, command: TextCommand) {
        self.commands.borrow_mut().push_back(command);
    }

    /// The session set by the view after the last input, frame or timer, with the commands
    /// applied by then, or None when no field accepts text. A composition that reads the session runs again
    /// when the session changes.
    #[must_use]
    pub fn session(&self) -> Option<TextSession> {
        self.session.get()
    }

    pub(crate) fn set_session(&self, session: Option<TextSession>) {
        self.session.set_if_changed(session);
    }

    /// The session as set last, read without making a composition depend on it.
    pub(crate) fn last_session(&self) -> Option<TextSession> {
        self.session.peek()
    }

    pub(crate) fn receive(&self) -> Option<TextCommand> {
        self.commands.borrow_mut().pop_front()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.commands.borrow().is_empty()
    }
}

/// Two senders are equal when they send to the same view.
impl PartialEq for TextSender {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.commands, &other.commands)
    }
}
