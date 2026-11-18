// SPDX-License-Identifier: GPL-3.0-only

use crate::buffer_importer::HardwareBufferImporter;
use crate::host::{AospError, AospLinks, HostLink};
use crate::native_window::NativeWindow;
use crate::output::DISPLAY;
use crate::presentation::{AospBackend, AospPresentation};
use crate::streams::AospStreams;
use lavashell_actor::SystemClock;
use lavashell_platform::{Capability, Connection, HostFacts, Platform};
use lavashell_vulkan_renderer::{VulkanBackend, VulkanRenderer};
use std::collections::BTreeSet;
use std::sync::Arc;

pub struct Aosp {
    link: HostLink,
}

impl Platform for Aosp {
    type Setup = HostLink;
    type Error = AospError;
    type Backend = AospBackend;
    type Control = NativeWindow;

    fn connect(link: HostLink) -> Result<Connection<Self>, AospError> {
        let backend: AospBackend = VulkanBackend {
            presentation: AospPresentation,
            streams: Box::new(|gpu: &VulkanRenderer| {
                HardwareBufferImporter::new(gpu).map(AospStreams::new)
            }),
        };
        Ok(Connection {
            platform: Self { link },
            backend,
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

    fn serve(self, links: AospLinks) -> Result<(), AospError> {
        self.link.serve(links)
    }
}
