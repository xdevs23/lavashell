// SPDX-License-Identifier: GPL-3.0-only
use crate::{Closed, Merge, Sender};
use std::sync::Arc;

/// Pushes items of type `U` into a mailbox whose items have another type, converting each item on the way. A clone
/// pushes into the same mailbox.
pub struct Outlet<U> {
    push: Arc<dyn Fn(U) -> Result<(), Closed> + Send + Sync>,
}

impl<U> Outlet<U> {
    /// # Errors
    ///
    /// Fails when the receiver of the mailbox is gone.
    pub fn push(&self, item: U) -> Result<(), Closed> {
        (self.push)(item)
    }
}

impl<U> Clone for Outlet<U> {
    fn clone(&self) -> Self {
        Self {
            push: Arc::clone(&self.push),
        }
    }
}

impl<T> Sender<T>
where
    T: Merge + Send + 'static,
    T::Key: Send,
{
    /// The outlet that pushes each item converted by `convert` into this mailbox.
    #[must_use]
    pub fn outlet<U: 'static>(
        &self,
        convert: impl Fn(U) -> T + Send + Sync + 'static,
    ) -> Outlet<U> {
        let sender = self.clone();
        Outlet {
            push: Arc::new(move |item| sender.push(convert(item))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mailbox;
    use crate::test_items::{Item, set};

    #[test]
    fn an_outlet_pushes_each_converted_item_into_the_mailbox() -> Result<(), Closed> {
        let (sender, receiver) = mailbox();
        let outlet = sender.outlet(|value| set(0, value));
        outlet.clone().push(1.0)?;
        outlet.push(2.0)?;
        assert_eq!(receiver.take()?, [Item::Set { key: 0, value: 2.0 }]);
        drop(receiver);
        assert_eq!(outlet.push(3.0), Err(Closed));
        Ok(())
    }
}
