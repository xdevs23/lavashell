// SPDX-License-Identifier: GPL-3.0-only
use super::{
    DragAction, DragActions, DragGesture, DragRequest, HashMap, SurfaceId, TextDropTarget,
    TransferId, View, text_data,
};
use lavashell_core::prelude::{NodeId, TextSelectionSource};
use std::{cell::RefCell, rc::Rc};

#[derive(Clone, Default)]
pub(super) struct NativeSources(Rc<RefCell<Sources>>);

#[derive(Default)]
struct Sources {
    active: Vec<Rc<RefCell<LocalSource>>>,
    linked: HashMap<TransferId, Option<Rc<RefCell<LocalSource>>>>,
}

/// The outcome of a received drop for a drag that may have started at a text selection in this app.
pub(super) enum LocalMove {
    /// The text moved from the field where the drag started to the target.
    Moved,
    /// The drop came from a drag source in this app, but its text cannot move: the drag already finished or moved
    /// the text, or the text no longer matches the selection. The drop is rejected.
    Refused,
    /// The drop did not come from a drag source in this app, or it was dropped outside the field where its drag
    /// started. It is received like any other drop.
    NotLocal,
}

struct LocalSource {
    device: u32,
    surface: SurfaceId,
    source: TextSelectionSource,
    submitted: bool,
    finished: bool,
    moved: bool,
}

impl NativeSources {
    pub fn request(&self, gesture: &DragGesture, device: u32, view: &View) -> Option<DragRequest> {
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
        let data = text_data(source.text())?;
        let local = Rc::new(RefCell::new(LocalSource {
            device,
            surface: gesture.surface,
            source,
            submitted: false,
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
            .on_start(move |view| {
                local.borrow_mut().submitted = true;
                starting.0.borrow_mut().active.push(local.clone());
                view.cancel_pending_text_drag();
            })
            .on_local_receive(move |id| {
                let source = receiving.borrow();
                if source.submitted
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
        if !source.submitted || source.finished || source.moved || source.source.text() != text {
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

    pub fn ambiguous(&self, device: u32, surface: SurfaceId, target: NodeId) -> bool {
        self.0.borrow().active.iter().any(|source| {
            let source = source.borrow();
            source.device == device
                && source.surface == surface
                && source.source.id() == target
                && source.submitted
                && !source.finished
        })
    }

    /// Opens the link for transfer `id`. A drag source in this app that sends the data of transfer `id` links
    /// itself to it through `on_local_receive`.
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

    /// Unlinks transfer `id` from the local drag source that it came from, if any.
    pub fn close_link(&self, id: TransferId) {
        self.0.borrow_mut().linked.retain(|linked, _| *linked != id);
    }
}
