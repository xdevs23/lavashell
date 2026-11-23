// SPDX-License-Identifier: GPL-3.0-only
use calloop::futures::ExecutorDestroyed;
use calloop::ping::{Ping, PingSource, make_ping};
use calloop::{EventLoop, LoopSignal};
use lavashell_actor::Outlet;
use lavashell_error_chain::message_chain;
use lavashell_error_chain::next_step::{NamesNextStep, NextStep};
use lavashell_event_loop::EventLoopFailure;
use lavashell_platform::PlatformEvent;
use std::pin::Pin;
use std::sync::mpsc;
use std::{io, thread};

mod panic_value;

pub use panic_value::PanicValue;

pub type ServiceWatch = Pin<Box<dyn Future<Output = ()>>>;

pub trait DesktopService: Send {
    fn into_watches(self: Box<Self>, view: &Outlet<PlatformEvent>) -> Vec<ServiceWatch>;
}

#[derive(Debug, thiserror::Error)]
pub enum ServicesStartError {
    #[error("the desktop services cannot create their stop signal")]
    CreateStop(#[source] io::Error),
    #[error("the desktop services cannot start their thread")]
    SpawnThread(#[source] io::Error),
    #[error("the desktop services cannot create their event loop")]
    CreateEventLoop(#[source] EventLoopFailure),
    #[error("the desktop services cannot create the executor that runs the watches")]
    CreateWatch(#[source] EventLoopFailure),
    #[error(
        "the desktop services cannot add the executor that runs the watches to their event loop"
    )]
    InsertWatches(#[source] EventLoopFailure),
    #[error("the desktop services cannot add their stop signal to their event loop")]
    InsertStop(#[source] EventLoopFailure),
    #[error(
        "the desktop services cannot start a watch that a service returns, because the executor \
         that runs the watches is gone; this is a defect in the desktop services"
    )]
    ScheduleWatch(#[source] ExecutorDestroyed),
    #[error(
        "the thread of the desktop services panicked with {value} before the watches started; \
         this is a defect in the desktop services"
    )]
    Panicked { value: PanicValue },
    #[error(
        "the thread of the desktop services ended without reporting whether the watches started; \
         this is a defect in the desktop services"
    )]
    EndedWithoutReport,
}

impl NamesNextStep for ServicesStartError {
    fn next_step(&self) -> NextStep {
        match self {
            Self::CreateStop(_) | Self::SpawnThread(_) => NextStep::FreeSystemResources,
            Self::CreateEventLoop(failure)
            | Self::CreateWatch(failure)
            | Self::InsertWatches(failure)
            | Self::InsertStop(failure) => failure.next_step(),
            Self::ScheduleWatch(_) | Self::Panicked { .. } | Self::EndedWithoutReport => {
                NextStep::ReportTheFailure
            }
        }
    }
}

struct ServicesStop(Ping);

impl Drop for ServicesStop {
    fn drop(&mut self) {
        self.0.ping();
    }
}

pub(crate) fn watch_services_while<T>(
    services: Vec<Box<dyn DesktopService>>,
    view: Outlet<PlatformEvent>,
    serve: impl FnOnce() -> T,
) -> Result<T, ServicesStartError> {
    let (stop, stopped) = make_ping().map_err(ServicesStartError::CreateStop)?;
    thread::scope(|scope| {
        let stop_on_return = ServicesStop(stop);
        let (started, start) = mpsc::sync_channel(0);
        let services_thread = thread::Builder::new()
            .name("desktop services".to_owned())
            .spawn_scoped(scope, move || {
                let watches = start_watches(services, &view, stopped);
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
            .map_err(ServicesStartError::SpawnThread)?;
        let Ok(started) = start.recv() else {
            return Err(match services_thread.join() {
                Err(payload) => ServicesStartError::Panicked {
                    value: PanicValue::of(payload.as_ref()),
                },
                Ok(()) => ServicesStartError::EndedWithoutReport,
            });
        };
        started?;
        let served = serve();
        drop(stop_on_return);
        if let Err(payload) = services_thread.join() {
            log::error!(
                "the thread of the desktop services panicked with {} while the app ran, and the \
                 app ran on without them; this is a defect in the desktop services. {}",
                PanicValue::of(payload.as_ref()),
                NextStep::ReportTheFailure
            );
        }
        Ok(served)
    })
}

fn start_watches(
    services: Vec<Box<dyn DesktopService>>,
    view: &Outlet<PlatformEvent>,
    stopped: PingSource,
) -> Result<EventLoop<'static, LoopSignal>, ServicesStartError> {
    let event_loop =
        EventLoop::try_new().map_err(|error| ServicesStartError::CreateEventLoop(error.into()))?;
    let handle = event_loop.handle();
    let (executor, scheduler) = calloop::futures::executor::<()>()
        .map_err(|error| ServicesStartError::CreateWatch(error.into()))?;
    let _watches_run_until_the_loop_ends = handle
        .insert_source(executor, |(), (), _| {})
        .map_err(|failure| ServicesStartError::InsertWatches(failure.into()))?;
    let _stop_runs_until_the_loop_ends = handle
        .insert_source(stopped, |(), (), signal: &mut LoopSignal| signal.stop())
        .map_err(|failure| ServicesStartError::InsertStop(failure.into()))?;
    for watch in services
        .into_iter()
        .flat_map(|service| service.into_watches(view))
    {
        scheduler
            .schedule(watch)
            .map_err(ServicesStartError::ScheduleWatch)?;
    }
    Ok(event_loop)
}

fn run_watches(mut event_loop: EventLoop<'static, LoopSignal>) {
    let mut signal = event_loop.get_signal();
    if let Err(error) = event_loop.run(None, &mut signal, |_| {}) {
        let failure = EventLoopFailure::from(error);
        log::error!(
            "the desktop services stop watching because their event loop failed: {}. {}",
            message_chain(&failure),
            failure.next_step()
        );
    }
}

#[cfg(test)]
mod tests;
