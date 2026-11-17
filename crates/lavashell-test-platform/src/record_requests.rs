// SPDX-License-Identifier: GPL-3.0-only
use lavashell_actor::Latest;
use std::sync::Arc;

#[derive(Clone, Default)]
pub struct RecordRequests(Arc<Latest<RecordRequest>>);

struct RecordRequest;

impl RecordRequests {
    pub fn request(&self) {
        self.0.publish(RecordRequest);
    }

    pub(crate) fn take(&self) -> bool {
        self.0.take().is_some()
    }
}
