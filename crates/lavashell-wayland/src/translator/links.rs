// SPDX-License-Identifier: GPL-3.0-only
use lavashell_actor::Outlet;
use lavashell_platform::PlatformEvent;

#[derive(Clone)]
pub(crate) struct ViewLink(Outlet<PlatformEvent>);

impl ViewLink {
    pub(crate) fn new(view: Outlet<PlatformEvent>) -> Self {
        Self(view)
    }

    pub(crate) fn report(&self, event: impl Into<PlatformEvent>) {
        let _stopped_view_ends_the_run = self.0.push(event.into());
    }
}
