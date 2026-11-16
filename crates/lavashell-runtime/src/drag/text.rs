// SPDX-License-Identifier: GPL-3.0-only
use super::{DragGesture, DragRequest, DropEvent};
use lavashell_core::{
    prelude::{TextDropTarget, View},
    transfer::{DragAction, DragEvent, DragOfferInfo, DropAcceptance, TextFormat, TransferId},
};
use lavashell_platform::DragDevice;
use std::collections::{HashMap, HashSet};

mod refusal;
mod source;

pub(super) use refusal::{DropAccepted, DropRefusal};
use source::{LocalMove, NativeSources};

#[derive(Default)]
pub(super) struct TextTransfers {
    accepted: HashSet<TransferId>,
    pending: HashMap<TransferId, Pending>,
    sources: NativeSources,
}

struct Pending {
    device: DragDevice,
    target: Option<TextDropTarget>,
}

impl TextTransfers {
    fn hold_drop(&mut self, id: TransferId, drop: Pending) {
        if self.pending.insert(id, drop).is_some() {
            log::debug!("transfer {id:?} dropped again, so its earlier drop is replaced");
        }
    }

    fn apply_received(
        &mut self,
        event: DropEvent,
        view: &mut View,
    ) -> Result<DropAccepted, DropRefusal> {
        let pending = (self.pending.remove(&event.id)).ok_or(DropRefusal::NotPending)?;
        let data = event.data.map_err(DropRefusal::NotReceived)?;
        let text = String::from_utf8(data.bytes).map_err(DropRefusal::NotUtf8)?;
        let target = pending.target.ok_or(DropRefusal::NoTarget)?;
        if event.action == DragAction::Move {
            match (self.sources).receive(event.id, event.surface, &target, &text, view) {
                LocalMove::Moved => return Ok(DropAccepted::Moved),
                LocalMove::Refused => return Err(DropRefusal::MoveRefused),
                LocalMove::NotLocal => {}
            }
            if self
                .sources
                .ambiguous(pending.device, event.surface, target.id())
            {
                return Err(DropRefusal::AmbiguousSource);
            }
        }
        (view.insert_text_at(&target, &text))
            .then_some(DropAccepted::Inserted)
            .ok_or(DropRefusal::InsertRefused)
    }
}

impl TextTransfers {
    pub(super) fn forget(&mut self, id: TransferId) {
        let _accepted = self.accepted.remove(&id);
        let _pending = self.pending.remove(&id);
        self.sources.close_link(id);
    }

    pub(super) fn drag(
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

    pub(super) fn has_pending(&self, id: TransferId) -> bool {
        self.pending.contains_key(&id)
    }

    pub(super) fn owns(&self, event: &DragEvent) -> bool {
        let id = event.id();
        self.accepted.contains(&id) || self.pending.contains_key(&id)
    }

    pub(super) fn receive(
        &mut self,
        event: DropEvent,
        view: &mut View,
    ) -> Result<DropAccepted, DropRefusal> {
        let id = event.id;
        let received = self.apply_received(event, view);
        self.sources.close_link(id);
        received
    }

    pub(super) fn source(&self, gesture: &DragGesture, view: &View) -> Option<DragRequest> {
        self.sources.request(gesture, view)
    }
}

fn text_acceptance(info: &DragOfferInfo, view: &View) -> Option<DropAcceptance> {
    let format = TextFormat::by_preference()
        .find(|format| {
            info.mime_types
                .iter()
                .any(|available| available == format.mime_type())
        })
        .filter(|_| view.text_drop_target(info.position).is_some())?;
    let action = info.actions.preferred()?;
    Some(DropAcceptance {
        mime_type: format.mime_type().to_string(),
        action,
    })
}

#[cfg(test)]
mod tests;
