// SPDX-License-Identifier: GPL-3.0-only
use super::ActionKind;
use std::slice;

/// The kinds of the actions that a node accepts, each once, in increasing order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize), serde(transparent))]
pub struct ActionKinds(Vec<ActionKind>);

impl ActionKinds {
    #[must_use]
    pub fn contains(&self, kind: &ActionKind) -> bool {
        self.0.binary_search(kind).is_ok()
    }

    /// Adds `kind`. Returns whether it was absent before.
    pub fn insert(&mut self, kind: ActionKind) -> bool {
        let absent = !self.contains(&kind);
        self.extend([kind]);
        absent
    }

    pub fn retain(&mut self, wanted: impl FnMut(&ActionKind) -> bool) {
        self.0.retain(wanted);
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn iter(&self) -> slice::Iter<'_, ActionKind> {
        self.0.iter()
    }
}

impl Extend<ActionKind> for ActionKinds {
    fn extend<I: IntoIterator<Item = ActionKind>>(&mut self, kinds: I) {
        for kind in kinds {
            if let Err(position) = self.0.binary_search(&kind) {
                self.0.insert(position, kind);
            }
        }
    }
}

impl FromIterator<ActionKind> for ActionKinds {
    fn from_iter<I: IntoIterator<Item = ActionKind>>(kinds: I) -> Self {
        let mut set = Self::default();
        set.extend(kinds);
        set
    }
}

impl<const N: usize> From<[ActionKind; N]> for ActionKinds {
    fn from(kinds: [ActionKind; N]) -> Self {
        kinds.into_iter().collect()
    }
}

impl<'a> IntoIterator for &'a ActionKinds {
    type Item = &'a ActionKind;
    type IntoIter = slice::Iter<'a, ActionKind>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for ActionKinds {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let kinds: Vec<ActionKind> = crate::encoding::sequence(deserializer)?;
        if kinds.is_sorted_by(|before, after| before < after) {
            Ok(Self(kinds))
        } else {
            Err(serde::de::Error::custom(
                "the action kinds of a node must increase, each once",
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ActionKind, ActionKinds};
    use crate::geometry::Axis;

    #[test]
    fn kinds_are_stored_in_increasing_order_each_once() {
        let mut kinds = ActionKinds::from([
            ActionKind::Scroll(Axis::Vertical),
            ActionKind::Activate,
            ActionKind::Focus,
        ]);
        assert!(!kinds.insert(ActionKind::Activate));
        assert!(kinds.insert(ActionKind::SetValue));
        assert!(kinds.contains(&ActionKind::SetValue));
        assert!(!kinds.contains(&ActionKind::Decrement));
        assert_eq!(
            kinds.iter().copied().collect::<Vec<_>>(),
            [
                ActionKind::Focus,
                ActionKind::Activate,
                ActionKind::SetValue,
                ActionKind::Scroll(Axis::Vertical),
            ]
        );
    }

    #[cfg(feature = "serde")]
    #[test]
    fn kinds_out_of_order_or_repeated_are_not_read() -> crate::test_error::TestResult {
        for kinds in [
            [ActionKind::Activate, ActionKind::Focus],
            [ActionKind::Focus, ActionKind::Focus],
        ] {
            let bytes = postcard::to_allocvec(&kinds.as_slice())?;
            assert!(postcard::from_bytes::<ActionKinds>(&bytes).is_err());
        }
        let increasing = [ActionKind::Focus, ActionKind::Activate];
        let bytes = postcard::to_allocvec(&increasing.as_slice())?;
        assert_eq!(
            postcard::from_bytes::<ActionKinds>(&bytes)?,
            ActionKinds::from(increasing)
        );
        Ok(())
    }
}
