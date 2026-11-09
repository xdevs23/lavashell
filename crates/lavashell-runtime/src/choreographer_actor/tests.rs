// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use lavashell_actor::{Receiver, mailbox};
use lavashell_core::scene::tree::{Change, SceneNodeId};
use std::thread;

const SURFACE: SurfaceId = SurfaceId(0);

fn remove(node: u64) -> ChoreographerInput {
    let id = SceneNodeId(node);
    let change = SceneItem::Change(Change::Remove { id });
    ChoreographerInput::Change(SURFACE, Box::new(change))
}

fn end(sequence: u64) -> ChoreographerInput {
    ChoreographerInput::End(SURFACE, Some(FrameSequence(sequence)))
}

fn handle(actor: &mut ChoreographerActor, inputs: impl IntoIterator<Item = ChoreographerInput>) {
    for input in inputs {
        assert_eq!(actor.handle(input), Ok(Flow::Continue));
    }
}

/// The surface, the item count and the end value of a frame sent to the scene builder.
type Summary = (SurfaceId, usize, Option<FrameSequence>);

fn summary((surface, frame): (SurfaceId, SceneFrame)) -> Summary {
    (surface, frame.items.len(), frame.end)
}

/// The summary of a frame for the surface with `items` items and the end value `sequence`.
fn frame_sent(items: usize, sequence: u64) -> Summary {
    (SURFACE, items, Some(FrameSequence(sequence)))
}

fn sent(builder: &Receiver<BuilderInput>) -> Result<Vec<Summary>, Closed> {
    let mut frames = Vec::new();
    for input in builder.take()? {
        if let BuilderInput::Frames(sent) = input {
            frames.extend(sent.into_iter().map(summary));
        }
    }
    Ok(frames)
}

#[test]
fn the_frames_reach_the_scene_builder_only_once_it_requested_them() -> Result<(), Closed> {
    let (builder, builder_inbox) = mailbox();
    let (_views, inbox) = mailbox();
    let mut actor = ChoreographerActor::new(builder, inbox.intake());
    handle(&mut actor, [end(1)]);
    assert_eq!(sent(&builder_inbox)?, [frame_sent(0, 1)]);
    handle(&mut actor, [end(2), end(3)]);
    handle(&mut actor, [ChoreographerInput::Ask(Vec::new())]);
    assert_eq!(sent(&builder_inbox)?, [frame_sent(0, 3)]);
    handle(&mut actor, [end(4)]);
    drop(actor);
    assert_eq!(sent(&builder_inbox), Err(Closed));
    Ok(())
}

#[test]
fn the_end_value_of_frames_dropped_by_a_restart_reaches_the_scene_builder() -> Result<(), Closed> {
    let (builder, builder_inbox) = mailbox();
    let (_views, inbox) = mailbox();
    let mut actor = ChoreographerActor::new(builder, inbox.intake());
    handle(&mut actor, [end(1)]);
    assert_eq!(sent(&builder_inbox)?, [frame_sent(0, 1)]);
    handle(&mut actor, [remove(1), end(2), remove(2)]);
    handle(&mut actor, [ChoreographerInput::Restart(SURFACE)]);
    handle(&mut actor, [ChoreographerInput::Ask(Vec::new())]);
    drop(actor);
    assert_eq!(sent(&builder_inbox)?, [frame_sent(0, 2)]);
    Ok(())
}

#[test]
fn the_views_wait_while_a_surface_has_more_waiting_items_than_nodes_until_the_builder_requests()
-> Result<(), Closed> {
    let (builder, builder_inbox) = mailbox();
    let (views, inbox) = mailbox();
    let mut actor = ChoreographerActor::new(builder, inbox.intake());
    handle(&mut actor, [end(1)]);
    assert_eq!(sent(&builder_inbox)?.len(), 1);
    handle(&mut actor, [ChoreographerInput::Ask(vec![(SURFACE, 1)])]);
    handle(&mut actor, [remove(1), remove(2), end(2)]);
    assert_eq!(sent(&builder_inbox)?.len(), 1);
    handle(&mut actor, [remove(3), remove(4), end(3)]);
    let waiting = views.clone();
    let view = thread::spawn(move || waiting.push(ChoreographerInput::Retire(SURFACE)));
    views.push(ChoreographerInput::Ask(Vec::new()))?;
    let passed = inbox.take()?;
    assert!(matches!(passed.as_slice(), [ChoreographerInput::Ask(_)]));
    handle(&mut actor, passed);
    assert_eq!(sent(&builder_inbox)?, [frame_sent(2, 3)]);
    assert!(matches!(
        inbox.take()?.as_slice(),
        [ChoreographerInput::Retire(SURFACE)]
    ));
    assert!(matches!(view.join(), Ok(Ok(()))));
    Ok(())
}

#[test]
fn a_node_count_from_before_a_restart_leaves_the_views_free_to_send() -> Result<(), Closed> {
    let (builder, builder_inbox) = mailbox();
    let (views, inbox) = mailbox();
    let mut actor = ChoreographerActor::new(builder, inbox.intake());
    handle(&mut actor, [end(1)]);
    assert_eq!(sent(&builder_inbox)?.len(), 1);
    handle(&mut actor, [ChoreographerInput::Restart(SURFACE)]);
    handle(&mut actor, [ChoreographerInput::Ask(vec![(SURFACE, 0)])]);
    handle(&mut actor, [remove(1), remove(2), end(2)]);
    assert_eq!(sent(&builder_inbox)?.len(), 1);
    handle(&mut actor, [remove(3), remove(4), end(3)]);
    views.push(remove(5))?;
    assert!(matches!(
        inbox.take()?.as_slice(),
        [ChoreographerInput::Change(..)]
    ));
    Ok(())
}
