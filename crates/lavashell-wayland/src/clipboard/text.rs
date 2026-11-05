// SPDX-License-Identifier: GPL-3.0-only
use super::pipes::Scope;
use crate::Error;
use crate::app::Platform;
use crate::text_transfer::{TEXT_FORMATS, text_data};
use lavashell_core::prelude::{Key, TextSelectionSource, View};
use lavashell_core::surface::SurfaceId;
use lavashell_core::transfer::TransferData;
use sctk::reexports::client::protocol::wl_seat::WlSeat;
use smithay_client_toolkit as sctk;

/// The action that the host runs for a clipboard shortcut instead of sending the key to the view.
pub(crate) enum ClipboardShortcut {
    Paste,
    Cut(TextSelectionSource, TransferData),
    Copy(String),
}

/// Returns the action that the host runs for Control with `key`, or `None` when the host sends the key to `view` as a
/// shortcut. Paste needs an editable focused field. Cut needs selected text that the focused field can give up. Copy
/// needs selected text.
pub(crate) fn clipboard_shortcut(view: &mut View, key: Key) -> Option<ClipboardShortcut> {
    match key {
        Key::Character('v') => view
            .editing_target()
            .is_some()
            .then_some(ClipboardShortcut::Paste),
        Key::Character('x') => cut_shortcut(view),
        Key::Character('c') => view.copy_selection().map(ClipboardShortcut::Copy),
        _ => None,
    }
}

fn cut_shortcut(view: &View) -> Option<ClipboardShortcut> {
    let source = view
        .text_selection_source(view.focus()?)
        .filter(TextSelectionSource::can_move)?;
    let data = text_data(source.text())?;
    Some(ClipboardShortcut::Cut(source, data))
}

impl Platform {
    pub(crate) fn copy_text(&mut self, text: &str) {
        if let Some(data) = text_data(text) {
            self.clipboard.queue.set_data(data, |result| {
                if let Err(error) = result {
                    log::warn!("the clipboard selection was not set: {error}");
                }
            });
        }
    }

    /// Sets `data` as the clipboard selection. When that succeeds, removes the selected text of `source` from the
    /// field it was recorded from.
    pub(crate) fn cut_text(
        &mut self,
        surface: SurfaceId,
        source: TextSelectionSource,
        data: TransferData,
    ) {
        let actions = self.config.view_actions.clone();
        self.clipboard
            .queue
            .set_data(data, move |result| match result {
                Ok(()) => actions.dispatch(surface, move |view| {
                    if !view.complete_text_move(&source) {
                        log::debug!("the cut text stays in its field, which changed meanwhile");
                    }
                }),
                Err(error) => log::warn!("the cut text stays in its field: {error}"),
            });
    }

    pub(crate) fn paste_selection(&mut self, surface: SurfaceId, seat: &WlSeat) {
        let Some(target) = self.clipboard_view(surface).and_then(|view| {
            view.editing_target()
                .and_then(|id| view.text_paste_target(id))
        }) else {
            return;
        };
        let Some(target_surface) = self.clipboard_surface_for(surface) else {
            return;
        };
        let Some(offer) = self
            .clipboard
            .devices
            .iter()
            .find(|device| &device.seat == seat)
            .and_then(|device| device.device.data().selection_offer())
        else {
            return;
        };
        let Some(mime_type) = offer.with_mime_types(|types| {
            TEXT_FORMATS
                .iter()
                .find(|format| types.iter().any(|available| available == **format))
                .map(|format| (*format).into())
        }) else {
            return;
        };
        let pipe = match offer.receive(mime_type) {
            Ok(pipe) => pipe,
            Err(error) => {
                log::warn!("the clipboard text was not received: {error}");
                return;
            }
        };
        self.receive_transfer(
            pipe,
            Scope {
                seat: seat.clone(),
                surface: Some(target_surface.clone()),
                source: None,
                incoming: None,
            },
            Box::new(move |platform, bytes| {
                let text =
                    bytes.and_then(|bytes| String::from_utf8(bytes).map_err(Error::DecodeText));
                match text {
                    Ok(text) => {
                        if let Some(view) = platform.clipboard_view_for(&target_surface) {
                            let text: String = text
                                .chars()
                                .filter(|c| !c.is_control() || matches!(c, '\n' | '\r' | '\t'))
                                .collect();
                            if !view.paste_text_at(&target, &text) {
                                log::debug!("the pasted text was dropped, since its field changed");
                            }
                        }
                    }
                    Err(error) => log::warn!("the clipboard text was not read: {error}"),
                }
            }),
        );
    }
}
