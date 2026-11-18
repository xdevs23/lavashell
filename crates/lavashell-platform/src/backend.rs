// SPDX-License-Identifier: GPL-3.0-only

pub trait Backend: Send + 'static {
    type Surface: Send + 'static;
    type Streams: 'static;
}
