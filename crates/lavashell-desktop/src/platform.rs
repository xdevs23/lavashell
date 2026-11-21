// SPDX-License-Identifier: GPL-3.0-only
use crate::layer::build_services;
use crate::services::{DesktopService, ServicesStartError, watch_services_while};
use lavashell_platform::{Capability, Connection, Platform, PlatformLinks};
use lavashell_store::Store;
use std::collections::BTreeSet;

pub struct DesktopPlatform<P> {
    display: P,
    services: Vec<Box<dyn DesktopService>>,
}

#[derive(Debug, thiserror::Error)]
pub enum DesktopPlatformError<E> {
    #[error(transparent)]
    Display(E),
    #[error("the desktop platform cannot start the desktop services")]
    StartServices(#[source] ServicesStartError),
}

impl<P: Platform<Setup = ()>> Platform for DesktopPlatform<P> {
    type Setup = Store;
    type Error = DesktopPlatformError<P::Error>;
    type Backend = P::Backend;
    type Control = P::Control;

    fn connect(registered: Store) -> Result<Connection<Self>, Self::Error> {
        let Connection {
            platform,
            backend,
            host,
            clock,
        } = P::connect(()).map_err(DesktopPlatformError::Display)?;
        Ok(Connection {
            platform: Self {
                display: platform,
                services: build_services(&registered),
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
        watch_services_while(self.services, view, || self.display.serve(links))
            .map_err(DesktopPlatformError::StartServices)?
            .map_err(DesktopPlatformError::Display)
    }
}

#[cfg(test)]
mod tests;
