// SPDX-License-Identifier: GPL-3.0-only
use crate::Error;
use crate::steps::Steps;
use lavashell::prelude::*;
use lavashell_test_platform::{HostScript, RecordedFrame};

impl<C: 'static, L: Clone + 'static, E: From<Error> + 'static> Steps<C, L, E> {
    pub fn play(self, app: App, density: Density) -> Result<Vec<(L, RecordedFrame)>, E> {
        let script = HostScript::new(density).record_paced(self.links.clone());
        self.record(app, script)
    }
}
