// SPDX-License-Identifier: GPL-3.0-only
use crate::copy::CopyKey;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tag {
    Control(CopyKey),
    SelectableText,
    StateReadout,
    TransferTarget,
}
