// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use lavashell_actor::SystemClock;
use lavashell_core::output::OutputRequest;
use lavashell_core::scene::tree::SceneTree;
use lavashell_core::surface::{SurfaceChange, SurfaceState};
use lavashell_platform::{Attach, SurfaceAlpha};
use std::convert::Infallible;
use std::sync::mpsc;

const SURFACE: SurfaceId = SurfaceId::numbered(1);

#[derive(Debug, PartialEq)]
enum Seen {
    Message(u32),
    Committed(SurfaceState),
    Released,
}

struct RecordingControl(mpsc::Sender<Seen>);

impl SurfaceControl for RecordingControl {
    type Error = Infallible;
    type Message = u32;

    fn receive(&mut self, message: u32) {
        let _test_gone = self.0.send(Seen::Message(message));
    }

    fn commit_state(&mut self, state: &SurfaceState) -> Result<(), Infallible> {
        let _test_gone = self.0.send(Seen::Committed(state.clone()));
        Ok(())
    }
}

impl Drop for RecordingControl {
    fn drop(&mut self) {
        let _test_gone = self.0.send(Seen::Released);
    }
}

#[derive(Debug, thiserror::Error)]
enum TestError {
    #[error("the surface controls could not start")]
    Start(#[from] SpawnError<ControlsError<Infallible>>),
    #[error("the surface controls failed")]
    Run(#[from] JoinError<ControlsError<Infallible>>),
    #[error("the surface controls stopped")]
    Stopped(#[from] Closed),
}

fn start() -> Result<
    (
        SurfaceControls<RecordingControl>,
        mpsc::Receiver<ControlsOutput>,
    ),
    TestError,
> {
    let (outputs, received) = mpsc::channel();
    let observer = move |output| outputs.send(output).map_err(|_unsent| Closed);
    let controls = SurfaceControls::start(&SystemClock::start(), Box::new(observer))?;
    Ok((controls, received))
}

fn attach(control: RecordingControl) -> SurfaceAttachment<RecordingControl, ()> {
    SurfaceAttachment::Attach(Box::new(Attach {
        surface: SURFACE,
        window: (),
        alpha: SurfaceAlpha::Opaque,
        output: OutputRequest::default(),
        hdr_metadata: None,
        control,
    }))
}

fn built_with(state: SurfaceState) -> Arc<BuiltSurface> {
    Arc::new(BuiltSurface {
        sequence: None,
        scene: SceneTree::default().snapshot(),
        state,
    })
}

#[test]
fn an_attached_control_receives_its_messages_and_states_until_its_surface_is_detached()
-> Result<(), TestError> {
    let (mut controls, outputs) = start()?;
    let (seen, received) = mpsc::channel();
    let mut state = SurfaceState::default();
    state.apply(SurfaceChange::InputRegion(None));
    let attachments = controls.attachments();
    attachments.push(attach(RecordingControl(seen)))?;
    attachments.push(SurfaceAttachment::Control {
        surface: SURFACE,
        message: 7,
    })?;
    controls.commit_state(SURFACE, built_with(state.clone()))?;
    attachments.push(SurfaceAttachment::Detach(SURFACE))?;
    controls.stop()?;
    let expected = [Seen::Message(7), Seen::Committed(state), Seen::Released];
    assert_eq!(received.try_iter().collect::<Vec<_>>(), expected);
    let expected = [
        ControlsOutput::Committed(SURFACE),
        ControlsOutput::Retired(SURFACE),
    ];
    assert_eq!(outputs.try_iter().collect::<Vec<_>>(), expected);
    Ok(())
}

#[test]
fn a_state_committed_to_a_surface_without_a_control_ends_the_controls_with_a_failure()
-> Result<(), TestError> {
    let (mut controls, outputs) = start()?;
    controls.commit_state(SURFACE, built_with(SurfaceState::default()))?;
    assert!(matches!(
        controls.stop(),
        Err(JoinError::Failed(ControlsError::UnknownSurface(SURFACE)))
    ));
    assert_eq!(outputs.try_iter().count(), 0);
    Ok(())
}
