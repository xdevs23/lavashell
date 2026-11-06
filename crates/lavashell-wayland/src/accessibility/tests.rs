// SPDX-License-Identifier: GPL-3.0-only
mod native;

use super::*;
use crate::test_error::TestError;
use accesskit::{Action, Node, NodeId, Role, TextDirection, TreeId, TreeInfo};

const WINDOW: NodeId = NodeId(1);
const BUTTON: NodeId = NodeId(2);
const FIELD: NodeId = NodeId(3);
/// The id of the first text run of the field. Each further run has the next id.
const FIRST_RUN: u64 = 4;

/// A text run of the field: its text, the characters at which a word starts, its direction, and whether the next run
/// continues its line.
struct Run {
    text: &'static str,
    word_starts: &'static [u8],
    direction: TextDirection,
    continues_line: bool,
}

impl Run {
    /// A left-to-right run that ends its line.
    fn line(text: &'static str, word_starts: &'static [u8]) -> Self {
        Self {
            text,
            word_starts,
            direction: TextDirection::LeftToRight,
            continues_line: false,
        }
    }
}

/// The tree of the window "Window" with the button "Start", which has the focus, and the text input "Text", whose text
/// consists of `runs`.
fn tree(runs: &[Run]) -> Result<TreeUpdate, TestError> {
    let mut window = Node::new(Role::Window);
    window.set_label("Window");
    window.set_children([BUTTON, FIELD]);
    let mut button = Node::new(Role::Button);
    button.set_label("Start");
    button.add_action(Action::Click);
    button.add_action(Action::Focus);
    let mut field = Node::new(Role::TextInput);
    field.set_label("Text");
    field.set_value(runs.iter().map(|run| run.text).collect::<String>());
    field.add_action(Action::SetTextSelection);
    field.add_action(Action::ReplaceSelectedText);
    let mut nodes = vec![(WINDOW, window), (BUTTON, button)];
    let mut continued = None;
    for (id, run) in (FIRST_RUN..).zip(runs) {
        let mut node = Node::new(Role::TextRun);
        node.set_value(run.text);
        let lengths = (run.text.chars()).map(|character| u8::try_from(character.len_utf8()));
        node.set_character_lengths(lengths.collect::<Result<Vec<_>, _>>()?);
        node.set_word_starts(run.word_starts);
        node.set_text_direction(run.direction);
        if let Some(previous) = continued {
            node.set_previous_on_line(previous);
        }
        if run.continues_line {
            node.set_next_on_line(NodeId(id + 1));
        }
        continued = run.continues_line.then_some(NodeId(id));
        field.push_child(NodeId(id));
        nodes.push((NodeId(id), node));
    }
    nodes.push((FIELD, field));
    Ok(TreeUpdate {
        nodes,
        tree: Some(TreeInfo::new(WINDOW)),
        tree_id: TreeId::ROOT,
        focus: BUTTON,
    })
}

#[test]
fn worker_requests_wake_idle_event_loop_and_activation_always_returns_full_tree()
-> Result<(), TestError> {
    use accesskit::{ActionHandler, ActivationHandler};
    use sctk::reexports::calloop::{EventLoop, channel};
    let mut loop_: EventLoop<Vec<AccessibilityEvent>> = EventLoop::try_new()?;
    let (sender, receiver) = channel::channel();
    let requests = loop_
        .handle()
        .insert_source(receiver, |event, (), events| match event {
            channel::Event::Msg(event) => events.push(event),
            channel::Event::Closed => assert_eq!(events.len(), 3, "the worker ended early"),
        })?;
    let full = tree(&[Run::line("a💡é", &[0])])?;
    let snapshot = Arc::new(Latest::new());
    snapshot.publish(full.clone());
    let worker = std::thread::spawn(move || {
        let mut activation = handlers::Activation::new(SurfaceId(7), sender.clone(), snapshot);
        assert_eq!(activation.request_initial_tree(), Some(full.clone()));
        assert_eq!(activation.request_initial_tree(), Some(full));
        handlers::Actions::new(SurfaceId(7), sender).do_action(accesskit::ActionRequest {
            action: Action::Click,
            target_tree: TreeId::ROOT,
            target_node: BUTTON,
            data: None,
        });
    });
    let mut events = Vec::new();
    while events.len() < 3 {
        loop_.dispatch(None, &mut events)?;
    }
    if let Err(panic) = worker.join() {
        std::panic::resume_unwind(panic);
    }
    assert!(matches!(
        events[0],
        AccessibilityEvent::Activate(SurfaceId(7))
    ));
    assert!(matches!(
        events[1],
        AccessibilityEvent::Activate(SurfaceId(7))
    ));
    assert!(matches!(
        events[2],
        AccessibilityEvent::Action(SurfaceId(7), _)
    ));
    loop_.handle().remove(requests);
    Ok(())
}
