// SPDX-License-Identifier: GPL-3.0-only
use crate::clock::VirtualClock;
use crate::session::Session;
use crate::{Error, FrameCapture, HostScript, RecordedFrames};
use lavashell_platform::{
    Capability, Connection, HostFacts, OutputId, Platform, PlatformLinks, no_streams,
};
use lavashell_renderer::{HeadlessPresentation, NoStreams};
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
    type Presentation = HeadlessPresentation;
    type Control = FrameCapture;
    type Streams = NoStreams;

    fn connect(host: ScriptedHost) -> Result<Connection<Self>, Error> {
        let clock = VirtualClock::default();
        Ok(Connection {
            presentation: HeadlessPresentation::with_surfaces(),
            streams: no_streams(),
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

    fn serve(self, links: PlatformLinks<FrameCapture, NoStreams>) -> Result<(), Error> {
        let ScriptedHost { script, recorded } = self.host;
        let mut session = Session::new(self.clock, links, script.density);
        let served = session.play(script.steps);
        let _test_gone = recorded.send(session.into_frames());
        served
    }
}
