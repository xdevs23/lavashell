// SPDX-License-Identifier: GPL-3.0-only
use lavashell_actor::{Closed, Merge, mailbox};

/// Pushes `items` into a new mailbox, one after the other, and returns what the receiver takes from it.
pub(crate) fn merged<T: Merge>(items: impl IntoIterator<Item = T>) -> Result<Vec<T>, Closed> {
    let (sender, receiver) = mailbox();
    for item in items {
        sender.push(item)?;
    }
    drop(sender);
    receiver.take()
}
