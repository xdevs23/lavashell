// SPDX-License-Identifier: GPL-3.0-only
use crate::clock::VirtualClock;
use crate::session::Session;
use crate::{Error, FrameCapture, HostScript, RecordedFrames};
use lavashell_platform::{
    Capability, Connection, HeadlessRendererBackend, HostFacts, OutputId, Platform, PlatformLinks,
};
use std::collections::BTreeSet;
use std::marker::PhantomData;
use std::sync::{Arc, mpsc};

const OUTPUT: OutputId = OutputId(0);

pub struct ScriptedHost {
    pub(crate) script: HostScript,
    pub(crate) recorded: mpsc::Sender<RecordedFrames>,
}

pub struct TestPlatform<B> {
    clock: VirtualClock,
    host: ScriptedHost,
    backend: PhantomData<B>,
}

impl<B: HeadlessRendererBackend> Platform for TestPlatform<B> {
    type Setup = ScriptedHost;
    type Error = Error;
    type Backend = B;
    type Control = FrameCapture;

    fn connect(host: ScriptedHost) -> Result<Connection<Self>, Error> {
        let clock = VirtualClock::default();
        Ok(Connection {
            backend: B::create(),
            host: HostFacts {
                outputs: vec![OUTPUT],
                reports_presentation: true,
            },
            clock: Arc::new(clock.clone()),
            platform: Self {
                clock,
                host,
                backend: PhantomData,
            },
        })
    }

    fn capabilities(&self) -> BTreeSet<Capability> {
        BTreeSet::new()
    }

    fn serve(self, links: PlatformLinks<FrameCapture, B>) -> Result<(), Error> {
        let ScriptedHost { script, recorded } = self.host;
        let mut session = Session::<B>::new(self.clock, links, script.density);
        let served = session.play(script.steps);
        let _test_gone = recorded.send(session.into_frames());
        served
    }
}
