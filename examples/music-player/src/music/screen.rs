// SPDX-License-Identifier: GPL-3.0-only
use super::Icons;
use lavashell_example_support::Layout;

#[derive(Clone, Copy)]
pub(super) struct Screen {
    pub(super) icons: Icons,
    pub(super) layout: Layout,
}
