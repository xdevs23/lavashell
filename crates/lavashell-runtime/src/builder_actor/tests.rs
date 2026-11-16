// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use lavashell_actor::mailbox;

fn observer() -> BuilderObserver {
    Box::new(|_report: BuilderReport| Ok(()))
}

#[test]
fn the_choreographer_stops_once_the_scene_builder_is_gone() {
    let (choreographer, inbox) = mailbox();
    drop(BuilderActor::new(Arc::default(), choreographer, observer()));
    let stopped = inbox.take();
    assert!(matches!(stopped.as_deref(), Ok([ChoreographerInput::Stop])));
}

#[test]
fn the_scene_builder_keeps_handling_its_inputs_once_the_choreographer_stopped()
-> Result<(), ApplyError> {
    let (choreographer, inbox) = mailbox();
    drop(inbox);
    let mut builder = BuilderActor::new(Arc::default(), choreographer, observer());
    let flow = builder.handle(BuilderInput::Frames(Vec::new()))?;
    assert!(matches!(flow, Flow::Continue));
    Ok(())
}
