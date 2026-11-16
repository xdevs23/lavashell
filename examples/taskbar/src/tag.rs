// SPDX-License-Identifier: GPL-3.0-only
use crate::{CopyKey, islands::Island, model::App};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tag {
    Bar,
    Track,
    Island(Island),
    Slot(App),
    Control(CopyKey),
}
