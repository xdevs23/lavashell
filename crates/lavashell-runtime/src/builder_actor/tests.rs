// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use lavashell_actor::mailbox;

#[test]
fn the_choreographer_stops_once_the_scene_builder_is_gone() {
    let (choreographer, inbox) = mailbox();
    let observer: BuilderObserver = Box::new(|_report: BuilderReport| Ok(()));
    drop(BuilderActor::new(Arc::default(), choreographer, observer));
    let stopped = inbox.take();
    assert!(matches!(stopped.as_deref(), Ok([ChoreographerInput::Stop])));
}
