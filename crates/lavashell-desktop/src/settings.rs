// SPDX-License-Identifier: GPL-3.0-only
use calloop::channel::Channel;
use calloop::futures::ExecutorDestroyed;
use calloop::{EventLoop, LoopSignal};
use lavashell_actor::Outlet;
use lavashell_desktop_settings::{DesktopSetting, DesktopSettingSource};
use lavashell_desktop_settings_freedesktop::AppearanceSource;
use lavashell_platform::{PlatformEvent, SettingsEvent};
use std::convert::Infallible;

#[derive(Debug, thiserror::Error)]
pub enum SettingsWatchError {
    #[error("the event loop of the desktop settings could not be created")]
    CreateEventLoop(#[source] calloop::Error),
    #[error("the watch of the desktop settings could not be created")]
    CreateWatch(#[source] calloop::Error),
    #[error("the desktop settings could not join their event loop")]
    JoinEventLoop(#[source] calloop::Error),
    #[error("the watch of a desktop setting could not be started")]
    ScheduleWatch(#[source] ExecutorDestroyed),
    #[error("the event loop of the desktop settings failed")]
    RunEventLoop(#[source] calloop::Error),
}

pub(crate) fn sources() -> Vec<Box<dyn DesktopSettingSource>> {
    vec![Box::new(AppearanceSource)]
}

fn settings_event(setting: DesktopSetting) -> SettingsEvent {
    match setting {
        DesktopSetting::Appearance(appearance) => SettingsEvent::Appearance(appearance),
    }
}

pub(crate) fn watch_settings(
    sources: Vec<Box<dyn DesktopSettingSource>>,
    view: &Outlet<PlatformEvent>,
    stopped: Channel<Infallible>,
) -> Result<(), SettingsWatchError> {
    let mut event_loop: EventLoop<LoopSignal> =
        EventLoop::try_new().map_err(SettingsWatchError::CreateEventLoop)?;
    let handle = event_loop.handle();
    let (executor, scheduler) =
        calloop::futures::executor::<()>().map_err(SettingsWatchError::CreateWatch)?;
    let _watches_run_until_the_loop_ends = handle
        .insert_source(executor, |(), (), _| {})
        .map_err(|failure| SettingsWatchError::JoinEventLoop(failure.error))?;
    let _stop_runs_until_the_loop_ends = handle
        .insert_source(stopped, |_closed, (), signal: &mut LoopSignal| {
            signal.stop();
        })
        .map_err(|failure| SettingsWatchError::JoinEventLoop(failure.error))?;
    for source in sources {
        let view = view.clone();
        scheduler
            .schedule(source.watch(Box::new(move |setting| {
                let _stopped_view_ends_the_run = view.push(settings_event(setting).into());
            })))
            .map_err(SettingsWatchError::ScheduleWatch)?;
    }
    let mut signal = event_loop.get_signal();
    event_loop
        .run(None, &mut signal, |_| {})
        .map_err(SettingsWatchError::RunEventLoop)
}

#[cfg(test)]
mod tests;
