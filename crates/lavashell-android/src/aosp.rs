// SPDX-License-Identifier: GPL-3.0-only
//! The platform of an AOSP host. `AospHost::serve` links it to the run started inside it, and the platform translates
//! the callbacks of the host into events for the runtime.

use crate::buffer_importer::HardwareBufferImporter;
use crate::host::{AospError, HostLink, take_served_link};
use crate::native_window::NativeWindow;
use crate::output::DISPLAY;
use crate::presentation::AospPresentation;
use crate::streams::AospStreams;
use lavashell_actor::SystemClock;
use lavashell_platform::{Capability, Connection, HostFacts, Platform, PlatformLinks, StreamSetup};
use lavashell_renderer::VulkanRenderer;
use std::collections::BTreeSet;
use std::sync::Arc;

/// The platform for the AOSP host attached to the thread that starts the run.
pub struct Aosp {
    link: HostLink,
}

impl Platform for Aosp {
    type Error = AospError;
    type Presentation = AospPresentation;
    type Control = NativeWindow;
    type Streams = AospStreams;

    fn connect() -> Result<Connection<Self>, AospError> {
        let link = take_served_link().ok_or(AospError::Unserved)?;
        let streams: StreamSetup<AospStreams> =
            Box::new(|gpu: &VulkanRenderer| HardwareBufferImporter::new(gpu).map(AospStreams::new));
        Ok(Connection {
            platform: Self { link },
            presentation: AospPresentation,
            streams,
            host: HostFacts {
                outputs: vec![DISPLAY],
                reports_presentation: false,
            },
            clock: Arc::new(SystemClock::start()),
        })
    }

    /// An AOSP host shows one window and offers none of the optional subsystems.
    fn capabilities(&self) -> BTreeSet<Capability> {
        BTreeSet::new()
    }

    fn serve(self, links: PlatformLinks<NativeWindow, AospStreams>) -> Result<(), AospError> {
        self.link.serve(links)
    }
}
