// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]
//! The messages between a host that shows a view inside a box and the guest process that provides it.

mod accessibility;
mod codec;
mod hello;
mod messages;
mod size;

pub use accessibility::AccessibilityUpdate;
pub use codec::{Account, Charge, Decoded, Encoded, Uncharged, decode, encode};
pub use hello::{Hello, Version, Versions};
pub use messages::{FrameSequence, GuestMessage, HostMessage, ProposalSequence, ResizeSequence};
pub use size::NonNegativeSize;

/// One number for every message, motions included, raised by each change that alters their bytes.
pub const VERSION: Version = Version(1);

#[cfg(test)]
type TestResult = Result<(), ProtocolError>;

#[derive(Debug, thiserror::Error)]
pub enum ProtocolError {
    #[error("cannot encode a message")]
    Encode(#[source] postcard::Error),
    #[error("cannot decode a message")]
    Decode(#[source] postcard::Error),
    #[error(
        "decoding a message needs {requested} more bytes of its account, which has {remaining} left"
    )]
    Charge { requested: usize, remaining: usize },
    #[error("a decode cannot charge the memory of {0} before it is allocated")]
    Uncharged(Uncharged),
    #[error("{trailing} bytes follow the end of a message")]
    Finish { trailing: usize },
    #[error("the versions from {oldest:?} to {newest:?} are in the wrong order")]
    Versions { oldest: Version, newest: Version },
}
