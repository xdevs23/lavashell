// SPDX-License-Identifier: GPL-3.0-only
use crate::drag::{DragGesture, DragRequest};
use lavashell_core::prelude::{NodeId, TextDropTarget, TextSelectionSource, View};
use lavashell_core::surface::SurfaceId;
use lavashell_core::transfer::{DragAction, DragActions, TransferData, TransferId};
use lavashell_platform::DragDevice;
use std::collections::HashMap;
use std::{cell::RefCell, rc::Rc};

#[derive(Clone, Default)]
pub(super) struct NativeSources(Rc<RefCell<Sources>>);

#[derive(Default)]
struct Sources {
    active: Vec<Rc<RefCell<LocalSource>>>,
    linked: HashMap<TransferId, Option<Rc<RefCell<LocalSource>>>>,
}

pub(super) enum LocalMove {
    Moved,
    Refused,
    NotLocal,
}

struct LocalSource {
    device: Option<DragDevice>,
    surface: SurfaceId,
    source: TextSelectionSource,
    finished: bool,
    moved: bool,
}

impl NativeSources {
    pub fn request(&self, gesture: &DragGesture, view: &View) -> Option<DragRequest> {
        let source = view.text_selection_source_at(gesture.origin)?;
        if view.text_source_contains(&source, gesture.position)
            || !view.text_source_drag_ready(&source, gesture.position)
        {
            return None;
        }
        let actions = DragActions::new(
            [DragAction::Copy, DragAction::Move]
                .into_iter()
                .filter(|action| *action == DragAction::Copy || source.can_move()),
        );
        let data = TransferData::text(source.text());
        let local = Rc::new(RefCell::new(LocalSource {
            device: None,
            surface: gesture.surface,
            source,
            finished: false,
            moved: false,
        }));
        let completing = local.clone();
        let sources = self.clone();
        let receiving = local.clone();
        let linked = self.clone();
        let starting = self.clone();
        Some(
            DragRequest::new(data, actions, move |result, view| {
                let mut source = completing.borrow_mut();
                source.finished = true;
                if !source.moved
                    && matches!(result, Ok(DragAction::Move))
                    && !view.complete_text_move(&source.source)
                {
                    log::debug!("the moved text stays in its field, which changed during the drag");
                }
                let mut sources = sources.0.borrow_mut();
                sources.active.retain(|item| !Rc::ptr_eq(item, &completing));
            })
            .on_start_with_device(move |device, view| {
                local.borrow_mut().device = Some(device);
                starting.0.borrow_mut().active.push(local.clone());
                view.cancel_pending_text_drag();
            })
            .on_local_receive(move |id| {
                let source = receiving.borrow();
                if source.device.is_some()
                    && !source.finished
                    && let Some(pending) = linked.0.borrow_mut().linked.get_mut(&id)
                {
                    *pending = Some(receiving.clone());
                }
            }),
        )
    }

    pub fn receive(
        &self,
        id: TransferId,
        surface: SurfaceId,
        target: &TextDropTarget,
        text: &str,
        view: &mut View,
    ) -> LocalMove {
        let Some(local) = self.0.borrow_mut().linked.remove(&id).flatten() else {
            return LocalMove::NotLocal;
        };
        let mut source = local.borrow_mut();
        if source.device.is_none()
            || source.finished
            || source.moved
            || source.source.text() != text
        {
            return LocalMove::Refused;
        }
        if source.surface != surface || source.source.id() != target.id() {
            return LocalMove::NotLocal;
        }
        source.moved = view.move_text_at(&source.source, target);
        if source.moved {
            LocalMove::Moved
        } else {
            LocalMove::Refused
        }
    }

    pub fn ambiguous(&self, device: DragDevice, surface: SurfaceId, target: NodeId) -> bool {
        self.0.borrow().active.iter().any(|source| {
            let source = source.borrow();
            source.device == Some(device)
                && source.surface == surface
                && source.source.id() == target
                && !source.finished
        })
    }

    pub fn open_link(&self, id: TransferId) {
        if self
            .0
            .borrow_mut()
            .linked
            .insert(id, None)
            .flatten()
            .is_some()
        {
            log::debug!(
                "transfer {id:?} opened its link again, so its earlier local source is unlinked"
            );
        }
    }

    pub fn close_link(&self, id: TransferId) {
        let _whether_linked_is_not_needed = self.0.borrow_mut().linked.remove(&id);
    }
}
