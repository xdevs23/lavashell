// SPDX-License-Identifier: GPL-3.0-only
use crate::settings::{SettingsWatchError, sources, watch_settings};
use lavashell_platform::{Capability, Connection, Platform, PlatformLinks};
use std::collections::BTreeSet;
use std::io;
use std::thread;

pub struct DesktopPlatform<P> {
    display: P,
}

#[derive(Debug, thiserror::Error)]
pub enum DesktopPlatformError<E> {
    #[error(transparent)]
    Display(E),
    #[error("the thread for the desktop settings could not be created")]
    SpawnSettings(#[source] io::Error),
    #[error("the desktop settings failed")]
    WatchSettings(#[source] SettingsWatchError),
    #[error("the thread for the desktop settings panicked")]
    SettingsPanicked,
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
            platform: Self { display: platform },
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
        thread::scope(|scope| {
            let (stop, stopped) = calloop::channel::channel();
            let settings = thread::Builder::new()
                .name("desktop settings".to_owned())
                .spawn_scoped(scope, move || watch_settings(sources(), &view, stopped))
                .map_err(DesktopPlatformError::SpawnSettings)?;
            let served = self.display.serve(links);
            drop(stop);
            let watched = match settings.join() {
                Ok(watched) => watched.map_err(DesktopPlatformError::WatchSettings),
                Err(_panic) => Err(DesktopPlatformError::SettingsPanicked),
            };
            served.map_err(DesktopPlatformError::Display).and(watched)
        })
    }
}
