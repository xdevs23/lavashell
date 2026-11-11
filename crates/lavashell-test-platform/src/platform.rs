// SPDX-License-Identifier: GPL-3.0-only
use crate::clock::VirtualClock;
use crate::session::Session;
use crate::{Error, FrameCapture, HostScript, RecordedFrames};
use lavashell_platform::{Capability, Connection, HostFacts, OutputId, Platform, PlatformLinks};
use lavashell_renderer::HeadlessPresentation;
use std::cell::RefCell;
use std::collections::BTreeSet;
use std::sync::{Arc, mpsc};

/// The one output of the test host.
const OUTPUT: OutputId = OutputId(0);

/// The script served by a test platform connected on this thread, and where it sends the frames it recorded.
pub(crate) struct Host {
    pub(crate) script: HostScript,
    pub(crate) recorded: mpsc::Sender<RecordedFrames>,
}

/// Sets the host of the test platforms that connect on this thread while it lives, and then restores the host set
/// before.
pub(crate) struct ScopedHost {
    previous: Option<Host>,
}

thread_local! {
    static HOST: RefCell<Option<Host>> = const { RefCell::new(None) };
}

impl ScopedHost {
    pub(crate) fn set(host: Host) -> Self {
        Self {
            previous: HOST.replace(Some(host)),
        }
    }
}

impl Drop for ScopedHost {
    fn drop(&mut self) {
        HOST.set(self.previous.take());
    }
}

/// A platform whose host is the script set by [`HostScript::serve`] on the thread that connects. Its surfaces are
/// windows that present headlessly, and the platform leaves every optional subsystem switched off.
pub struct TestPlatform {
    clock: VirtualClock,
    host: Host,
}

impl Platform for TestPlatform {
    type Error = Error;
    type Presentation = HeadlessPresentation;
    type Control = FrameCapture;

    fn connect() -> Result<Connection<Self>, Error> {
        let host = HOST.with_borrow_mut(Option::take).ok_or(Error::NoScript)?;
        let clock = VirtualClock::default();
        Ok(Connection {
            presentation: HeadlessPresentation::with_surfaces(),
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

    fn serve(self, links: PlatformLinks<FrameCapture>) -> Result<(), Error> {
        let Host { script, recorded } = self.host;
        let mut session = Session::new(self.clock, links, script.density);
        let served = session.play(script.steps);
        let _test_gone = recorded.send(session.into_frames());
        served
    }
}
