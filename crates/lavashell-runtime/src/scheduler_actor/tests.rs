// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::scheduler::{FrameKind, ViewState};
use lavashell_core::frame::FrameTime;
use lavashell_platform::TimingEvent;
use std::sync::mpsc;
use std::time::Duration;

const WINDOW: SurfaceId = SurfaceId(1);

#[derive(Debug, thiserror::Error)]
enum TestError {
    #[error("the scheduler could not start or stop")]
    Scheduler(#[from] Error),
    #[error("the scheduler is gone")]
    Send(#[from] Closed),
    #[error("the scheduler sent no further output")]
    Outputs(#[from] mpsc::RecvError),
}

fn wants(view: ViewState, deadline: Option<Duration>) -> SchedulerInput {
    SchedulerInput::Wants {
        surface: WINDOW,
        view,
        deadline,
    }
}

fn display_ready() -> SchedulerInput {
    SchedulerInput::Timing(TimingEvent::DisplayReady(WINDOW))
}

fn published(next_change: Option<Duration>) -> SchedulerInput {
    SchedulerInput::Published {
        surface: WINDOW,
        next_change,
    }
}

#[test]
fn pacing_inputs_merge_and_a_view_report_follows_the_inputs_sent_before_it() -> Result<(), Closed> {
    let (sender, inbox) = mailbox();
    let pushed = [
        published(Some(Duration::from_secs(1))),
        display_ready(),
        published(None),
        wants(ViewState::Unchanged, None),
        display_ready(),
        wants(ViewState::Changed, None),
    ];
    for input in pushed {
        sender.push(SchedulerItem::Input(input))?;
    }
    let taken = inbox.take()?;
    assert!(matches!(
        taken.as_slice(),
        [
            SchedulerItem::Input(SchedulerInput::Published {
                next_change: None,
                ..
            }),
            SchedulerItem::Input(SchedulerInput::Timing(TimingEvent::DisplayReady(WINDOW))),
            SchedulerItem::Input(SchedulerInput::Wants {
                view: ViewState::Unchanged,
                ..
            }),
            SchedulerItem::Input(SchedulerInput::Timing(TimingEvent::DisplayReady(WINDOW))),
            SchedulerItem::Input(SchedulerInput::Wants {
                view: ViewState::Changed,
                ..
            }),
        ]
    ));
    Ok(())
}

#[test]
fn the_observer_receives_the_started_frame_and_the_wake_time() -> Result<(), TestError> {
    let (seen, outputs) = mpsc::channel();
    let observer: SchedulerObserver =
        Box::new(move |output| seen.send(output).map_err(|_unsent| Closed));
    let mut thread = SchedulerThread::start(Redraw::OnChange, Instant::now(), observer)?;
    thread.send(SchedulerInput::Opened(WINDOW))?;
    thread.send(wants(ViewState::Changed, Some(Duration::MAX)))?;
    assert!(matches!(
        outputs.recv()?,
        SchedulerOutput::Frame {
            surface: WINDOW,
            kind: FrameKind::Compose,
            ..
        }
    ));
    assert!(matches!(
        outputs.recv()?,
        SchedulerOutput::WakeAt(wake) if wake == Some(Duration::MAX)
    ));
    thread.stop()?;
    Ok(())
}

#[test]
fn a_stop_follows_the_outputs_of_the_inputs_sent_before() -> Result<(), TestError> {
    let (seen, outputs) = mpsc::channel();
    let observer: SchedulerObserver =
        Box::new(move |output| seen.send(output).map_err(|_unsent| Closed));
    let mut thread = SchedulerThread::start(Redraw::OnChange, Instant::now(), observer)?;
    thread.send(SchedulerInput::Opened(WINDOW))?;
    let discarded = TimingEvent::Discarded {
        surface: WINDOW,
        frame: FrameTime::default(),
    };
    thread.send(SchedulerInput::Timing(discarded))?;
    thread.stop()?;
    assert!(matches!(outputs.recv()?, SchedulerOutput::Report(_)));
    Ok(())
}
