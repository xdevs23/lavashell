// SPDX-License-Identifier: GPL-3.0-only
use crate::{Error, RecordingPlatform};
use lavashell::prelude::App;
use lavashell_example_support::{FrameLog, reject_failures};
use lavashell_test_platform::{HostScript, RecordedFrames, ScriptedHost};
use std::{cell::RefCell, rc::Rc};

pub(crate) fn record(app: App, script: HostScript) -> Result<RecordedFrames, Error> {
    serve(script, |host| {
        reject_component_failures(app, |app| {
            let (app, log) = FrameLog::start(app);
            on_recording_platform(app, host)?;
            Ok(log.write()?)
        })
    })
}

pub(crate) fn record_unlogged(app: App, script: HostScript) -> Result<RecordedFrames, Error> {
    serve(script, |host| {
        reject_component_failures(app, |app| on_recording_platform(app, host))
    })
}

fn serve(
    script: HostScript,
    run: impl FnOnce(ScriptedHost) -> Result<(), Error>,
) -> Result<RecordedFrames, Error> {
    let (ran, frames) = script.serve(run);
    ran?;
    frames.map_err(Error::NotServed)
}

fn on_recording_platform(app: App, host: ScriptedHost) -> Result<(), Error> {
    lavashell_runtime::run::<RecordingPlatform>(app, host).map_err(Error::Record)
}

fn reject_component_failures(
    app: App,
    run: impl FnOnce(App) -> Result<(), Error>,
) -> Result<(), Error> {
    let failures = Rc::new(RefCell::new(Vec::new()));
    let observed = Rc::clone(&failures);
    run(app.failure_observer(move |_, failure| observed.borrow_mut().push(failure)))?;
    Ok(reject_failures(failures.take())?)
}
