// SPDX-License-Identifier: GPL-3.0-only
use crate::clock::VirtualClock;
use crate::session::Session;
use crate::{Error, FrameCapture, HostScript, RecordedFrames};
use lavashell_platform::{Capability, Connection, HostFacts, OutputId, Platform, PlatformLinks};
use lavashell_vulkan_renderer::HeadlessBackend;
use std::collections::BTreeSet;
use std::sync::{Arc, mpsc};

const OUTPUT: OutputId = OutputId(0);

pub struct ScriptedHost {
    pub(crate) script: HostScript,
    pub(crate) recorded: mpsc::Sender<RecordedFrames>,
}

pub struct TestPlatform {
    clock: VirtualClock,
    host: ScriptedHost,
}

impl Platform for TestPlatform {
    type Setup = ScriptedHost;
    type Error = Error;
    type Backend = HeadlessBackend;
    type Control = FrameCapture;

    fn connect(host: ScriptedHost) -> Result<Connection<Self>, Error> {
        let clock = VirtualClock::default();
        Ok(Connection {
            backend: HeadlessBackend::with_surfaces(),
            host: HostFacts {
                outputs: vec![OUTPUT],
                reports_presentation: true,
            },
            clock: Arc::new(clock.clone()),
            platform: Self { clock, host },
        })
    }

    fn capabilities(&self) -> BTreeSet<Capability> {
        BTreeSet::new()
    }

    fn serve(self, links: PlatformLinks<FrameCapture, HeadlessBackend>) -> Result<(), Error> {
        let ScriptedHost { script, recorded } = self.host;
        let mut session = Session::new(self.clock, links, script.density);
        let served = session.play(script.steps);
        let _test_gone = recorded.send(session.into_frames());
        served
    }
}
