// SPDX-License-Identifier: GPL-3.0-only
use lavashell_desktop::{DesktopServices, ServicesStartError};
use lavashell_error_chain::next_step::{NamesNextStep, NextStep};
use lavashell_platform::{Capability, Connection, Platform, PlatformLinks};
use lavashell_store::Store;
use std::collections::BTreeSet;

pub struct DisplayWithDesktop<D> {
    display: D,
    services: DesktopServices,
}

#[derive(Debug, thiserror::Error)]
pub enum DisplayWithDesktopError<E> {
    #[error(transparent)]
    Display(E),
    #[error("the desktop services cannot start")]
    StartServices(#[source] ServicesStartError),
}

impl<E: NamesNextStep> NamesNextStep for DisplayWithDesktopError<E> {
    fn next_step(&self) -> NextStep {
        match self {
            Self::Display(error) => error.next_step(),
            Self::StartServices(error) => error.next_step(),
        }
    }
}

impl<D: Platform<Setup = ()>> Platform for DisplayWithDesktop<D> {
    type Setup = Store;
    type Error = DisplayWithDesktopError<D::Error>;
    type Backend = D::Backend;
    type Control = D::Control;

    fn connect(registered: Store) -> Result<Connection<Self>, Self::Error> {
        let Connection {
            platform,
            backend,
            host,
            clock,
        } = D::connect(()).map_err(DisplayWithDesktopError::Display)?;
        Ok(Connection {
            platform: Self {
                display: platform,
                services: DesktopServices::set_up(&registered),
            },
            backend,
            host,
            clock,
        })
    }

    fn capabilities(&self) -> BTreeSet<Capability> {
        self.display.capabilities()
    }

    fn serve(self, links: PlatformLinks<D::Control, D::Backend>) -> Result<(), Self::Error> {
        let view = links.reports.view.clone();
        (self.services)
            .watch_while(view, || self.display.serve(links))
            .map_err(DisplayWithDesktopError::StartServices)?
            .map_err(DisplayWithDesktopError::Display)
    }
}

#[cfg(test)]
mod tests;
