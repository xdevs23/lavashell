// SPDX-License-Identifier: GPL-3.0-only
use crate::CopyKey;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tag {
    Control(CopyKey),
    Motion,
    OutputInfo,
    Stage,
}
