// SPDX-License-Identifier: GPL-3.0-only

use skia_safe::{Canvas, gpu::PurgeResourceOptions};
use std::collections::HashSet;

#[derive(Default)]
pub(super) struct UploadBatch {
    images: HashSet<u64>,
    bytes: usize,
}

impl UploadBatch {
    fn would_exceed(&self, id: u64, bytes: usize, limit: usize) -> bool {
        !self.images.contains(&id)
            && !self.images.is_empty()
            && self.bytes.saturating_add(bytes) > limit
    }

    fn record(&mut self, id: u64, bytes: usize) {
        if self.images.insert(id) {
            self.bytes = self.bytes.saturating_add(bytes);
        }
    }

    pub fn before_draw(&mut self, canvas: &Canvas, id: u64, bytes: usize) {
        let Some(mut context) = canvas.direct_context() else {
            return;
        };
        if self.would_exceed(id, bytes, context.resource_cache_limit()) {
            let _context_returned_for_chaining = context.flush_submit_and_sync_cpu();
            let _context_returned_for_chaining =
                context.purge_unlocked_resources(PurgeResourceOptions::AllResources);
            self.images.clear();
            self.bytes = 0;
        }
        self.record(id, bytes);
    }
}

#[cfg(any(test, feature = "test-support"))]
impl UploadBatch {
    pub fn pending(&self) -> crate::test_readings::TileUploads {
        crate::test_readings::TileUploads {
            images: self.images.len(),
            bytes: self.bytes,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distinct_pending_uploads_hit_the_byte_limit_but_reuse_does_not() {
        let limit = 64 * 1024;
        for bytes in [4, 16 * 1024] {
            let mut pending = UploadBatch::default();
            let mut batches = 0;
            for id in (0_u64..).take(2 * limit / bytes) {
                if pending.would_exceed(id, bytes, limit) {
                    assert!(pending.bytes <= limit);
                    batches += 1;
                    pending = UploadBatch::default();
                }
                pending.record(id, bytes);
                assert!(!pending.would_exceed(id, bytes, limit));
                pending.record(id, bytes);
                assert_eq!(pending.bytes, pending.images.len() * bytes);
            }
            assert!(batches > 0);
        }
    }
}
