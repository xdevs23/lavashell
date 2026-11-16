// SPDX-License-Identifier: GPL-3.0-only

use crate::keyboard::Key;
use crate::reactive::State;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TextAction {
    Search,
    Done,
    Next,
    Go,
    Send,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TextPurpose {
    #[default]
    Text,
    Number,
    Phone,
    Email,
    Url,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Capitalization {
    #[default]
    None,
    Sentences,
    Words,
    Characters,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TextHints {
    pub action: Option<TextAction>,
    pub purpose: TextPurpose,
    pub capitalization: Capitalization,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SessionId(pub u64);

impl SessionId {
    #[must_use]
    pub fn next(self) -> Self {
        Self(self.0 + 1)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct FieldPresses(pub u64);

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TextSession {
    pub id: SessionId,
    pub presses: FieldPresses,
    pub hints: TextHints,
    pub multiline: bool,
    pub text: FieldText,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum FieldText {
    Protected,
    Unknown,
    BeforeCursor(
        #[cfg_attr(feature = "serde", serde(with = "crate::encoding::shared_str"))] Arc<str>,
    ),
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TextCommand {
    Commit(String),
    DeleteBackward,
    Key(Key),
    Action(TextAction),
}

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

    #[must_use]
    pub fn session(&self) -> Option<TextSession> {
        self.session.get()
    }

    pub(crate) fn set_session(&self, session: Option<TextSession>) {
        self.session.set_if_changed(session);
    }

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

impl PartialEq for TextSender {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.commands, &other.commands)
    }
}
