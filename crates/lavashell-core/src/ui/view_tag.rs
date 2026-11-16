// SPDX-License-Identifier: GPL-3.0-only
use std::any::Any;
use std::fmt;
use std::sync::Arc;

pub trait ViewTag: fmt::Debug + Eq + Send + Sync + 'static {}

impl<T: fmt::Debug + Eq + Send + Sync + 'static> ViewTag for T {}

trait ErasedTag: fmt::Debug + Send + Sync {
    fn as_any(&self) -> &dyn Any;

    fn equals(&self, other: &dyn Any) -> bool;
}

impl<T: ViewTag> ErasedTag for T {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn equals(&self, other: &dyn Any) -> bool {
        other.downcast_ref::<T>() == Some(self)
    }
}

#[derive(Clone)]
pub struct NodeTag(Arc<dyn ErasedTag>);

impl NodeTag {
    pub fn new(tag: impl ViewTag) -> Self {
        Self(Arc::new(tag))
    }

    #[must_use]
    pub fn is<T: ViewTag>(&self, tag: &T) -> bool {
        self.0.equals(tag)
    }

    #[must_use]
    pub fn get<T: ViewTag>(&self) -> Option<&T> {
        self.0.as_any().downcast_ref()
    }
}

impl PartialEq for NodeTag {
    fn eq(&self, other: &Self) -> bool {
        self.0.equals(other.0.as_any())
    }
}

impl Eq for NodeTag {}

impl fmt::Debug for NodeTag {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

#[cfg(test)]
mod tests;
