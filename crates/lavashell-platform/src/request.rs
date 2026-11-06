// SPDX-License-Identifier: GPL-3.0-only
use crate::{InputRequest, WindowControlRequest};

/// What the runtime asks a platform to do after an input reached a view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlatformRequest {
    Input(InputRequest),
    WindowControl(WindowControlRequest),
}
