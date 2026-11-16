// SPDX-License-Identifier: GPL-3.0-only
use crate::clipboard::{ClipboardTransfers, TextClipboard};
use crate::drag::Drags;
use crate::surface_records::SurfaceRecords;
use crate::view_actions::ActionQueue;
use crate::watches::Watches;
use lavashell_core::surface::SurfaceId;
use lavashell_platform::{Capability, PlatformRequest};
use std::collections::BTreeSet;

pub(super) struct Transfers {
    pub(super) clipboard: ClipboardTransfers,
    pub(super) text: Option<TextClipboard>,
    pub(super) drags: Drags,
    pub(super) watches: Watches,
}

impl Transfers {
    pub(super) fn new(
        clipboard: ClipboardTransfers,
        mut drags: Drags,
        watches: Watches,
        action_queue: &ActionQueue,
        offered: &BTreeSet<Capability>,
    ) -> Self {
        drags.restrict_to(offered);
        Self {
            text: clipboard.text_clipboard(action_queue.view_actions(), offered),
            clipboard,
            drags,
            watches,
        }
    }

    pub(super) fn submit(&mut self, records: &mut SurfaceRecords) -> Vec<PlatformRequest> {
        self.clipboard.submit(records);
        self.drags.submit(records);
        let clipboard = self.clipboard.take_requests().into_iter();
        let targets = self.drags.take_target_requests().into_iter();
        let sources = self.drags.take_source_requests().into_iter();
        (clipboard.map(PlatformRequest::Clipboard))
            .chain(targets.map(PlatformRequest::DragTarget))
            .chain(sources.map(PlatformRequest::DragSource))
            .collect()
    }

    pub(super) fn forget_surface(&mut self, surface: SurfaceId, records: &mut SurfaceRecords) {
        self.drags.forget_surface(surface, records);
        self.clipboard.forget_surface(surface);
    }

    pub(super) fn close(&mut self, records: &mut SurfaceRecords) {
        self.drags.close(records);
        self.clipboard.close();
        self.watches.close();
    }
}
