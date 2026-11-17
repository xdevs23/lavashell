// SPDX-License-Identifier: GPL-3.0-only

use crate::buffer_importer::HardwareBufferImporter;
use crate::host::{AospError, HostLink};
use crate::native_window::NativeWindow;
use crate::output::DISPLAY;
use crate::presentation::AospPresentation;
use crate::streams::AospStreams;
use lavashell_actor::SystemClock;
use lavashell_platform::{Capability, Connection, HostFacts, Platform, PlatformLinks, StreamSetup};
use lavashell_renderer::VulkanRenderer;
use std::collections::BTreeSet;
use std::sync::Arc;

pub struct Aosp {
    link: HostLink,
}

impl Platform for Aosp {
    type Setup = HostLink;
    type Error = AospError;
    type Presentation = AospPresentation;
    type Control = NativeWindow;
    type Streams = AospStreams;

    fn connect(link: HostLink) -> Result<Connection<Self>, AospError> {
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

    fn capabilities(&self) -> BTreeSet<Capability> {
        BTreeSet::new()
    }

    fn serve(self, links: PlatformLinks<NativeWindow, AospStreams>) -> Result<(), AospError> {
        self.link.serve(links)
    }
}
