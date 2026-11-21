// SPDX-License-Identifier: GPL-3.0-only
use crate::settings::{SettingsStartError, sources, watch_settings_while};
use lavashell_desktop_settings::DesktopSettingSource;
use lavashell_platform::{Capability, Connection, Platform, PlatformLinks};
use std::collections::BTreeSet;
use std::sync::Arc;

pub struct DesktopPlatform<P> {
    display: P,
    sources: Vec<Arc<dyn DesktopSettingSource>>,
}

#[derive(Debug, thiserror::Error)]
pub enum DesktopPlatformError<E> {
    #[error(transparent)]
    Display(E),
    #[error("the desktop platform cannot start the desktop settings")]
    StartSettings(#[source] SettingsStartError),
}

impl<P: Platform> Platform for DesktopPlatform<P> {
    type Setup = P::Setup;
    type Error = DesktopPlatformError<P::Error>;
    type Backend = P::Backend;
    type Control = P::Control;

    fn connect(setup: P::Setup) -> Result<Connection<Self>, Self::Error> {
        let Connection {
            platform,
            backend,
            host,
            clock,
        } = P::connect(setup).map_err(DesktopPlatformError::Display)?;
        Ok(Connection {
            platform: Self {
                display: platform,
                sources: sources(),
            },
            backend,
            host,
            clock,
        })
    }

    fn capabilities(&self) -> BTreeSet<Capability> {
        self.display.capabilities()
    }

    fn serve(self, links: PlatformLinks<P::Control, P::Backend>) -> Result<(), Self::Error> {
        let view = links.reports.view.clone();
        watch_settings_while(self.sources, view, || self.display.serve(links))
            .map_err(DesktopPlatformError::StartSettings)?
            .map_err(DesktopPlatformError::Display)
    }
}

#[cfg(test)]
mod tests;
