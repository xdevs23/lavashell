// SPDX-License-Identifier: GPL-3.0-only
use super::{Clipboard, Error, log_failure};
use crate::view_actions::ViewActions;
use lavashell_core::keyboard::{Key, Shortcut};
use lavashell_core::surface::SurfaceId;
use lavashell_core::transfer::{TextFormat, TransferData};
use lavashell_core::view::{InputEvent, TextSelectionSource, View};

enum ClipboardShortcut {
    Paste,
    Cut(TextSelectionSource),
    Copy(String),
}

fn clipboard_shortcut(view: &mut View, key: Key) -> Option<ClipboardShortcut> {
    match key {
        Key::Character('v') => view
            .editing_target()
            .is_some()
            .then_some(ClipboardShortcut::Paste),
        Key::Character('x') => view
            .text_selection_source(view.focus()?)
            .filter(TextSelectionSource::can_move)
            .map(ClipboardShortcut::Cut),
        Key::Character('c') => view.copy_selection().map(ClipboardShortcut::Copy),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ShortcutOutcome {
    Applied,
    Declined,
}

pub(crate) struct TextClipboard {
    clipboard: Clipboard,
    actions: ViewActions,
}

impl TextClipboard {
    #[must_use]
    pub fn new(clipboard: Clipboard, actions: ViewActions) -> Self {
        Self { clipboard, actions }
    }

    pub fn apply_shortcut(
        &self,
        surface: SurfaceId,
        input: &InputEvent,
        view: &mut View,
    ) -> ShortcutOutcome {
        let InputEvent::Shortcut(Shortcut {
            key,
            control: true,
            alt: false,
            ..
        }) = *input
        else {
            return ShortcutOutcome::Declined;
        };
        let Some(shortcut) = clipboard_shortcut(view, key) else {
            return ShortcutOutcome::Declined;
        };
        match shortcut {
            ClipboardShortcut::Paste => self.paste_text(surface, view),
            ClipboardShortcut::Cut(source) => self.cut_text(surface, source),
            ClipboardShortcut::Copy(text) => self.copy_text(&text),
        }
        ShortcutOutcome::Applied
    }

    fn copy_text(&self, text: &str) {
        self.clipboard.set_data(TransferData::text(text), |result| {
            if let Err(error) = result {
                log_failure("the clipboard selection was not set", &error);
            }
        });
    }

    fn cut_text(&self, surface: SurfaceId, source: TextSelectionSource) {
        let actions = self.actions.clone();
        let data = TransferData::text(source.text());
        self.clipboard.set_data(data, move |result| match result {
            Ok(()) => actions.dispatch(surface, move |view| {
                if !view.complete_text_move(&source) {
                    log::debug!("the cut text was not removed because the field changed meanwhile");
                }
            }),
            Err(error) => log_failure("the cut text was not removed from the field", &error),
        });
    }

    fn paste_text(&self, surface: SurfaceId, view: &View) {
        let target = (view.editing_target()).and_then(|id| view.text_paste_target(id));
        let Some(target) = target else {
            return;
        };
        let actions = self.actions.clone();
        let formats = TextFormat::by_preference().map(TextFormat::mime_type);
        self.clipboard.read(surface, formats, move |data| {
            let text =
                data.and_then(|data| String::from_utf8(data.bytes).map_err(Error::DecodeText));
            match text {
                Ok(text) => actions.dispatch(surface, move |view| {
                    if !view.paste_text_at(&target, &without_control_characters(&text)) {
                        log::debug!("the pasted text was dropped because the field changed");
                    }
                }),
                Err(error) => log_failure("the clipboard text was not pasted", &error),
            }
        });
    }
}

fn without_control_characters(text: &str) -> String {
    (text.chars())
        .filter(|c| !c.is_control() || matches!(c, '\n' | '\r' | '\t'))
        .collect()
}

#[cfg(test)]
mod tests;
