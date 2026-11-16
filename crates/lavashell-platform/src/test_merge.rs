// SPDX-License-Identifier: GPL-3.0-only
use lavashell_actor::{Closed, Merge, mailbox};

pub(crate) fn merged<T: Merge>(items: impl IntoIterator<Item = T>) -> Result<Vec<T>, Closed> {
    let (sender, receiver) = mailbox();
    for item in items {
        sender.push(item)?;
    }
    drop(sender);
    receiver.take()
}
