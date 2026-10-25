// SPDX-License-Identifier: GPL-3.0-only
//! The first message in each direction of a connection, before the messages of the version both sides use.

use crate::ProtocolError;
use serde::{Deserialize, Serialize};

/// A version of the messages after the hello.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Version(pub u32);

/// The versions of the messages that a side reads and writes, from the oldest to the newest.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "VersionsParts")]
pub struct Versions {
    oldest: Version,
    newest: Version,
}

impl Versions {
    /// `None` when `oldest` is newer than `newest`.
    #[must_use]
    pub fn new(oldest: Version, newest: Version) -> Option<Self> {
        (oldest <= newest).then_some(Self { oldest, newest })
    }

    #[must_use]
    pub fn oldest(self) -> Version {
        self.oldest
    }

    #[must_use]
    pub fn newest(self) -> Version {
        self.newest
    }
}

#[derive(Deserialize)]
struct VersionsParts {
    oldest: Version,
    newest: Version,
}

impl TryFrom<VersionsParts> for Versions {
    type Error = ProtocolError;

    fn try_from(VersionsParts { oldest, newest }: VersionsParts) -> Result<Self, ProtocolError> {
        Self::new(oldest, newest).ok_or(ProtocolError::Versions { oldest, newest })
    }
}

/// The hello never changes, so a peer of any version can read it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hello {
    pub versions: Versions,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Account, TestResult, decode, encode};

    #[test]
    fn versions_whose_oldest_is_newer_than_the_newest_are_not_built_or_read() -> TestResult {
        assert!(Versions::new(Version(1), Version(2)).is_some());
        assert!(Versions::new(Version(2), Version(1)).is_none());
        let bytes = encode(&(2_u32, 1_u32))?;
        assert!(matches!(
            decode::<Versions>(bytes.bytes(), &mut Account::new(0)),
            Err(ProtocolError::Decode(_))
        ));
        Ok(())
    }
}
