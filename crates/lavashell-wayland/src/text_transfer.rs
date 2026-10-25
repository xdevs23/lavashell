// SPDX-License-Identifier: GPL-3.0-only
use crate::{App, DragEvent, DragGesture, DragOfferInfo, DragRequest, DropEvent, TransferId};
use lavashell_core::{
    prelude::{TextDropTarget, View},
    transfer::{DragAction, DragActions, DropAcceptance, Representation, TransferData},
};
use std::{collections::HashMap, sync::Arc};

mod source;

use source::{LocalMove, NativeSources};

pub(crate) const TEXT_FORMATS: [&str; 2] = ["text/plain;charset=utf-8", "text/plain"];

#[derive(Default)]
pub(crate) struct TextTransfers {
    accepted: HashMap<TransferId, Accepted>,
    pending: HashMap<TransferId, Pending>,
    sources: NativeSources,
}

struct Accepted {
    surface: usize,
    device: u32,
    choice: DropAcceptance,
}

struct Pending {
    surface: usize,
    device: u32,
    choice: DropAcceptance,
    target: Option<TextDropTarget>,
}

impl App {
    /// Enables built-in drag and drop of text: dragging selected text out of a text field and dropping text into a
    /// text field. The drag and drop callbacks of the app run first. The built-in handling applies only to the
    /// drags that the app did not accept.
    #[must_use]
    pub fn text_transfers(mut self) -> Self {
        if self.config.text_transfers.is_none() {
            self.config.text_transfers = Some(TextTransfers::default());
        }
        self
    }
}

impl TextTransfers {
    pub fn application_accepted(&mut self, event: &DragEvent) {
        self.forget(event_id(event));
    }

    /// Ends the built-in handling of transfer `id` by removing its acceptance, its pending drop and its link to a
    /// drag source in this app.
    fn forget(&mut self, id: TransferId) {
        self.set_acceptance(id, None);
        self.pending.retain(|pending, _| *pending != id);
        self.sources.close_link(id);
    }

    /// Replaces the acceptance of transfer `id` with the one computed for the latest enter or motion of the drag,
    /// or removes it for `None`.
    fn set_acceptance(&mut self, id: TransferId, acceptance: Option<Accepted>) {
        match acceptance {
            Some(acceptance) => self.accepted.extend([(id, acceptance)]),
            None => self.accepted.retain(|accepted, _| *accepted != id),
        }
    }

    /// Stores the drop of transfer `id` until its data arrives. A second drop of the same transfer replaces the
    /// first.
    fn hold_drop(&mut self, id: TransferId, drop: Pending) {
        if self.pending.insert(id, drop).is_some() {
            log::debug!("transfer {id:?} dropped again, so its earlier drop is replaced");
        }
    }

    pub fn drag(&mut self, event: &DragEvent, device: u32, view: &View) -> Option<DropAcceptance> {
        match event {
            DragEvent::Enter(info) | DragEvent::Motion(info) => {
                let choice = text_acceptance(info, view);
                let accepted = choice.clone().map(|choice| Accepted {
                    surface: info.surface,
                    device,
                    choice,
                });
                self.set_acceptance(info.id, accepted);
                choice
            }
            DragEvent::Drop(info) => {
                let accepted = self.accepted.remove(&info.id)?;
                let target = (accepted.surface == info.surface
                    && accepted.device == device
                    && info.selected_action == Some(accepted.choice.action))
                .then(|| view.text_drop_target(info.position))
                .flatten();
                self.sources.open_link(info.id);
                self.hold_drop(
                    info.id,
                    Pending {
                        surface: info.surface,
                        device: accepted.device,
                        choice: accepted.choice,
                        target,
                    },
                );
                None
            }
            DragEvent::Leave { id, .. } => {
                self.set_acceptance(*id, None);
                None
            }
            DragEvent::Cancelled { id, .. } => {
                self.forget(*id);
                None
            }
        }
    }

    pub fn has_pending(&self, id: TransferId) -> bool {
        self.pending.contains_key(&id)
    }

    pub fn owns(&self, event: &DragEvent) -> bool {
        let id = event_id(event);
        self.accepted.contains_key(&id) || self.pending.contains_key(&id)
    }

    pub fn receive(&mut self, event: DropEvent, view: &mut View) -> bool {
        let id = event.id;
        let accepted = self.apply_received(event, view);
        self.sources.close_link(id);
        accepted
    }

    fn apply_received(&mut self, event: DropEvent, view: &mut View) -> bool {
        let Some(pending) = self.pending.remove(&event.id) else {
            return false;
        };
        if event.surface != pending.surface || event.action != pending.choice.action {
            return false;
        }
        let Some(data) = (event.data)
            .inspect_err(|error| log::warn!("the dropped text was not received: {error}"))
            .ok()
        else {
            return false;
        };
        if data.mime_type != pending.choice.mime_type {
            return false;
        }
        let Some(text) = String::from_utf8(data.bytes)
            .inspect_err(|error| log::warn!("the dropped text is not UTF-8: {error}"))
            .ok()
        else {
            return false;
        };
        let Some(target) = pending.target else {
            return false;
        };
        if event.action == DragAction::Move {
            match (self.sources).receive(event.id, event.surface, &target, &text, view) {
                LocalMove::Moved => return true,
                LocalMove::Refused => return false,
                LocalMove::NotLocal => {}
            }
            if self
                .sources
                .ambiguous(pending.device, event.surface, target.id())
            {
                return false;
            }
        }
        view.insert_text_at(&target, &text)
    }

    pub fn source(&self, gesture: &DragGesture, device: u32, view: &View) -> Option<DragRequest> {
        self.sources.request(gesture, device, view)
    }
}

/// The text format and the action with which `view` accepts the drag, or `None` when the drag offers no text
/// format or no text field of `view` is at the position of the drag. Move is preferred over copy.
fn text_acceptance(info: &DragOfferInfo, view: &View) -> Option<DropAcceptance> {
    let mime_type = (TEXT_FORMATS.iter())
        .find(|format| {
            info.mime_types
                .iter()
                .any(|available| available == **format)
        })
        .filter(|_| view.text_drop_target(info.position).is_some())?;
    let action = [DragAction::Move, DragAction::Copy]
        .into_iter()
        .find(|action| info.actions.contains(*action))?;
    Some(DropAcceptance {
        mime_type: (*mime_type).to_string(),
        action,
    })
}

/// Transfer data that contains `text` in every format of `TEXT_FORMATS`, or `None` when `TransferData` rejects the
/// text. The reason for the rejection is logged.
pub(crate) fn text_data(text: &str) -> Option<TransferData> {
    let bytes: Arc<[u8]> = text.as_bytes().into();
    let representations = TEXT_FORMATS.map(|format| Representation::new(format, bytes.clone()));
    (representations.into_iter().collect::<Result<Vec<_>, _>>())
        .and_then(TransferData::new)
        .inspect_err(|error| log::warn!("the text cannot be transferred: {error}"))
        .ok()
}

fn event_id(event: &DragEvent) -> TransferId {
    match event {
        DragEvent::Enter(info) | DragEvent::Motion(info) | DragEvent::Drop(info) => info.id,
        DragEvent::Leave { id, .. } | DragEvent::Cancelled { id, .. } => *id,
    }
}
