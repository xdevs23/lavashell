// SPDX-License-Identifier: GPL-3.0-only
use std::rc::Rc;

/// A value that a composition builds again, compared with the value built before it. Shared handles and callbacks
/// compare by identity, as the values that a callback captured cannot be compared.
pub(crate) trait BuiltLike {
    /// Whether this value is known to equal `earlier`.
    fn is_built_like(&self, earlier: &Self) -> bool;
}

impl<T: ?Sized> BuiltLike for Rc<T> {
    fn is_built_like(&self, earlier: &Self) -> bool {
        Rc::ptr_eq(self, earlier)
    }
}

impl<T: BuiltLike> BuiltLike for Option<T> {
    fn is_built_like(&self, earlier: &Self) -> bool {
        match (self, earlier) {
            (Some(value), Some(earlier)) => value.is_built_like(earlier),
            (None, None) => true,
            (Some(_), None) | (None, Some(_)) => false,
        }
    }
}

impl<T: BuiltLike> BuiltLike for [T] {
    fn is_built_like(&self, earlier: &Self) -> bool {
        self.len() == earlier.len()
            && (self.iter().zip(earlier)).all(|(value, earlier)| value.is_built_like(earlier))
    }
}

impl<T: BuiltLike> BuiltLike for Vec<T> {
    fn is_built_like(&self, earlier: &Self) -> bool {
        self.as_slice().is_built_like(earlier)
    }
}
