// SPDX-License-Identifier: GPL-3.0-only
use super::{DragGesture, DragRequest, DropEvent};
use lavashell_core::{
    prelude::{TextDropTarget, View},
    transfer::{DragAction, DragEvent, DragOfferInfo, DropAcceptance, TEXT_FORMATS, TransferId},
};
use lavashell_platform::DragDevice;
use std::collections::{HashMap, HashSet};

mod source;

use source::{LocalMove, NativeSources};

/// The built-in drag and drop of text, which drags selected text out of a text field and drops text into a field.
#[derive(Default)]
pub(super) struct TextTransfers {
    /// The drags that the text drag accepted for their latest enter or motion.
    accepted: HashSet<TransferId>,
    pending: HashMap<TransferId, Pending>,
    sources: NativeSources,
}

struct Pending {
    device: DragDevice,
    target: Option<TextDropTarget>,
}

impl TextTransfers {
    pub fn application_accepted(&mut self, event: &DragEvent) {
        self.forget(event.id());
    }

    /// Ends the built-in handling of transfer `id` by removing its acceptance, its pending drop and its link to a
    /// drag source in this app.
    pub fn forget(&mut self, id: TransferId) {
        let _accepted = self.accepted.remove(&id);
        let _pending = self.pending.remove(&id);
        self.sources.close_link(id);
    }

    /// Stores the drop of transfer `id` until its data arrives. A second drop of the same transfer replaces the
    /// first.
    fn hold_drop(&mut self, id: TransferId, drop: Pending) {
        if self.pending.insert(id, drop).is_some() {
            log::debug!("transfer {id:?} dropped again, so its earlier drop is replaced");
        }
    }

    pub fn drag(
        &mut self,
        event: &DragEvent,
        device: DragDevice,
        view: &View,
    ) -> Option<DropAcceptance> {
        match event {
            DragEvent::Enter(info) | DragEvent::Motion(info) => {
                let choice = text_acceptance(info, view);
                let _changed = if choice.is_some() {
                    self.accepted.insert(info.id)
                } else {
                    self.accepted.remove(&info.id)
                };
                choice
            }
            DragEvent::Drop(info) => {
                if !self.accepted.remove(&info.id) {
                    return None;
                }
                self.sources.open_link(info.id);
                let target = view.text_drop_target(info.position);
                self.hold_drop(info.id, Pending { device, target });
                None
            }
            DragEvent::Leave { id, .. } => {
                let _left = self.accepted.remove(id);
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
        let id = event.id();
        self.accepted.contains(&id) || self.pending.contains_key(&id)
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
        let Some(data) = (event.data)
            .inspect_err(|error| log::warn!("the dropped text was not received: {error}"))
            .ok()
        else {
            return false;
        };
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

    pub fn source(&self, gesture: &DragGesture, view: &View) -> Option<DragRequest> {
        self.sources.request(gesture, view)
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

#[cfg(test)]
mod tests;
