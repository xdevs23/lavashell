// SPDX-License-Identifier: GPL-3.0-only
use crate::error::Error;
use crate::repository::{CommitId, RefUpdate};

// https://github.com/git/git/blob/v2.55.0/Documentation/githooks.adoc#L240
pub fn read_ref_updates(pushed_refs: &str) -> Result<Vec<RefUpdate>, Error> {
    pushed_refs
        .lines()
        .filter_map(|line| match line.split(' ').collect::<Vec<_>>()[..] {
            [_local_ref, local_object, _remote_ref, remote_object] => {
                let replaced = CommitId::existing(remote_object);
                CommitId::existing(local_object).map(|pushed| Ok(RefUpdate { pushed, replaced }))
            }
            _ => Some(Err(Error::PushedRef {
                line: line.to_owned(),
            })),
        })
        .collect()
}
