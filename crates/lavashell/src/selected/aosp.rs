// SPDX-License-Identifier: GPL-3.0-only
pub use lavashell_aosp as aosp;

use crate::RunError;
use lavashell_aosp::HostLink;
use lavashell_runtime::App;

pub type Platform = lavashell_aosp::Aosp;

pub fn run(app: App, link: HostLink) -> Result<(), RunError> {
    lavashell_runtime::run::<Platform>(app, link)
}
