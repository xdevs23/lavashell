// SPDX-License-Identifier: GPL-3.0-only
use calloop::futures::ExecutorDestroyed;
use calloop::ping::{Ping, PingSource, make_ping};
use calloop::{EventLoop, LoopSignal};
use lavashell_actor::Outlet;
use lavashell_desktop_settings::{DesktopSetting, DesktopSettingSource};
use lavashell_desktop_settings_freedesktop::AppearanceSource;
use lavashell_error_chain::message_chain;
use lavashell_platform::{PlatformEvent, SettingsEvent};
use std::sync::mpsc;
use std::{io, thread};

#[derive(Debug, thiserror::Error)]
pub enum SettingsStartError {
    #[error("the desktop settings cannot create their stop signal")]
    CreateStop(#[source] io::Error),
    #[error("the desktop settings cannot start their thread")]
    SpawnThread(#[source] io::Error),
    #[error("the desktop settings cannot create their event loop")]
    CreateEventLoop(#[source] calloop::Error),
    #[error("the desktop settings cannot create the executor of their watches")]
    CreateWatch(#[source] calloop::Error),
    #[error("the desktop settings cannot add the executor of their watches to their event loop")]
    InsertWatches(#[source] calloop::Error),
    #[error("the desktop settings cannot add their stop signal to their event loop")]
    InsertStop(#[source] calloop::Error),
    #[error("the desktop settings cannot start the watch of a desktop setting")]
    ScheduleWatch(#[source] ExecutorDestroyed),
    #[error("the thread of the desktop settings panicked before the watches started")]
    Panicked,
}

pub(crate) fn sources() -> Vec<Box<dyn DesktopSettingSource>> {
    vec![Box::new(AppearanceSource)]
}

fn settings_event(setting: DesktopSetting) -> SettingsEvent {
    match setting {
        DesktopSetting::Appearance(appearance) => SettingsEvent::Appearance(appearance),
    }
}

struct SettingsStop(Ping);

impl Drop for SettingsStop {
    fn drop(&mut self) {
        self.0.ping();
    }
}

pub(crate) fn watch_settings_while<T>(
    sources: Vec<Box<dyn DesktopSettingSource>>,
    view: Outlet<PlatformEvent>,
    serve: impl FnOnce() -> T,
) -> Result<T, SettingsStartError> {
    let (stop, stopped) = make_ping().map_err(SettingsStartError::CreateStop)?;
    thread::scope(|scope| {
        let _stop_on_return = SettingsStop(stop);
        let (started, start) = mpsc::sync_channel(0);
        let settings = thread::Builder::new()
            .name("desktop settings".to_owned())
            .spawn_scoped(scope, move || {
                let watches = start_watches(sources, &view, stopped);
                drop(view);
                match watches {
                    Ok(event_loop) => {
                        let _serve_waits_for_the_start = started.send(Ok(()));
                        run_watches(event_loop);
                    }
                    Err(error) => {
                        let _serve_waits_for_the_start = started.send(Err(error));
                    }
                }
            })
            .map_err(SettingsStartError::SpawnThread)?;
        let Ok(started) = start.recv() else {
            let _payload_reported_as_panicked = settings.join();
            return Err(SettingsStartError::Panicked);
        };
        started?;
        Ok(serve())
    })
}

fn start_watches(
    sources: Vec<Box<dyn DesktopSettingSource>>,
    view: &Outlet<PlatformEvent>,
    stopped: PingSource,
) -> Result<EventLoop<'static, LoopSignal>, SettingsStartError> {
    let event_loop = EventLoop::try_new().map_err(SettingsStartError::CreateEventLoop)?;
    let handle = event_loop.handle();
    let (executor, scheduler) =
        calloop::futures::executor::<()>().map_err(SettingsStartError::CreateWatch)?;
    let _watches_run_until_the_loop_ends = handle
        .insert_source(executor, |(), (), _| {})
        .map_err(|failure| SettingsStartError::InsertWatches(failure.error))?;
    let _stop_runs_until_the_loop_ends = handle
        .insert_source(stopped, |(), (), signal: &mut LoopSignal| signal.stop())
        .map_err(|failure| SettingsStartError::InsertStop(failure.error))?;
    for source in sources {
        let view = view.clone();
        scheduler
            .schedule(source.watch(Box::new(move |setting| {
                let _stopped_view_ends_the_run = view.push(settings_event(setting).into());
            })))
            .map_err(SettingsStartError::ScheduleWatch)?;
    }
    Ok(event_loop)
}

fn run_watches(mut event_loop: EventLoop<'static, LoopSignal>) {
    let mut signal = event_loop.get_signal();
    if let Err(error) = event_loop.run(None, &mut signal, |_| {}) {
        log::error!(
            "the desktop settings stop watching because their event loop failed: {}",
            message_chain(error)
        );
    }
}

#[cfg(test)]
mod tests;
