// SPDX-License-Identifier: GPL-3.0-only
//! Tests that make AT-SPI D-Bus calls on a private bus against a fixture of the accessibility registry, without using
//! the bus of the desktop session.

use super::*;
use sctk::reexports::calloop::{EventLoop, channel};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use zbus::blocking::{Connection, MessageIterator, Proxy, connection::Builder};
use zbus::zvariant::OwnedObjectPath;

type Address = (String, OwnedObjectPath);

/// The requests that the bridge sent to the event loop, and whether the bridge closed the channel of those requests.
#[derive(Default)]
struct Requests {
    events: Vec<Event>,
    closed: bool,
}

struct Status(Arc<AtomicBool>);

#[zbus::interface(name = "org.a11y.Status")]
impl Status {
    #[zbus(property)]
    fn is_enabled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }

    #[zbus(property)]
    fn set_is_enabled(&self, enabled: bool) {
        self.0.store(enabled, Ordering::SeqCst);
    }

    #[zbus(property)]
    fn screen_reader_enabled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

const ROOT: &str = "/org/a11y/atspi/accessible/root";

struct Registry {
    embedded: mpsc::Sender<Address>,
    name: String,
    root: OwnedObjectPath,
}

/// The failure of a call to the registry fixture.
#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "org.lavashell.test")]
enum RegistryError {
    #[zbus(error)]
    Bus(zbus::Error),
    /// The test ended and dropped the receiver of the embedded addresses.
    EmbedsReceiverGone,
}

#[zbus::interface(name = "org.a11y.atspi.Socket")]
impl Registry {
    fn embed(&self, plug: Address) -> Result<(Address,), RegistryError> {
        if self.embedded.send(plug).is_err() {
            return Err(RegistryError::EmbedsReceiverGone);
        }
        Ok(((self.name.clone(), self.root.clone()),))
    }
}

#[test]
fn private_bus_atspi_roundtrip() -> Result<(), TestError> {
    crate::test_bus::run(
        "LAVASHELL_PRIVATE_ATSPI_CHILD",
        "accessibility::tests::native::private_bus_atspi_roundtrip",
        child,
    )
}

fn proxy<'a>(
    connection: &'a Connection,
    address: &'a Address,
    interface: &'a str,
) -> Result<Proxy<'a>, TestError> {
    Ok(Proxy::new(
        connection,
        address.0.as_str(),
        address.1.as_str(),
        interface,
    )?)
}

fn wait_event(
    loop_: &mut EventLoop<Requests>,
    requests: &mut Requests,
    predicate: impl Fn(&Event) -> bool,
) -> Result<Event, TestError> {
    loop {
        if let Some(index) = requests.events.iter().position(&predicate) {
            return Ok(requests.events.remove(index));
        }
        if requests.closed {
            return Err(TestError::RequestsClosed);
        }
        loop_.dispatch(None, requests)?;
    }
}

/// Subscribes to every signal on the bus. A test subscribes before it makes the change that it waits for, so the signal
/// of that change cannot be missed.
fn signals(client: &Connection) -> Result<MessageIterator, TestError> {
    let rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .build();
    Ok(MessageIterator::for_match_rule(rule, client, None)?)
}

/// Reads a value with `read`, and reads it again after each signal on the bus until `holds` returns true for it.
/// Returns that value.
fn wait_for<T>(
    signals: &mut MessageIterator,
    mut read: impl FnMut() -> Result<T, TestError>,
    holds: impl Fn(&T) -> bool,
) -> Result<T, TestError> {
    loop {
        let value = read()?;
        if holds(&value) {
            return Ok(value);
        }
        let signal = signals.next().ok_or(TestError::BusClosed)?;
        assert_eq!(signal?.message_type(), zbus::message::Type::Signal);
    }
}

fn children(connection: &Connection, address: &Address) -> Result<Vec<Address>, TestError> {
    Ok(proxy(connection, address, "org.a11y.atspi.Accessible")?.call("GetChildren", &())?)
}

/// Starts the fixtures of the accessibility bus status and of the registry on the bus at `address`. Returns their
/// connections and the receiver of the addresses embedded by the registry.
fn serve_fixture(
    address: &str,
) -> Result<(Connection, Connection, mpsc::Receiver<Address>), TestError> {
    let (embedded, embeds) = mpsc::channel();
    let status = Builder::address(address)?
        .name("org.a11y.Bus")?
        .serve_at("/org/a11y/bus", Status(Arc::new(AtomicBool::new(true))))?
        .build()?;
    let registry = Builder::address(address)?
        .name("org.a11y.atspi.Registry")?
        .build()?;
    let name = registry
        .unique_name()
        .ok_or(TestError::NoUniqueName)?
        .to_string();
    let served = registry.object_server().at(
        ROOT,
        Registry {
            embedded,
            name,
            root: OwnedObjectPath::try_from(ROOT)?,
        },
    )?;
    assert!(served);
    Ok((status, registry, embeds))
}

fn child() -> Result<(), TestError> {
    let address = std::env::var("DBUS_SESSION_BUS_ADDRESS")?;
    let (_status, _registry, embeds) = serve_fixture(&address)?;
    let client = Builder::address(address.as_str())?.build()?;
    let mut signals = signals(&client)?;
    let mut loop_: EventLoop<Requests> = EventLoop::try_new()?;
    let (sender, receiver) = channel::channel();
    let source = loop_
        .handle()
        .insert_source(receiver, |event, (), requests| match event {
            channel::Event::Msg(event) => requests.events.push(event),
            channel::Event::Closed => requests.closed = true,
        })?;
    let mut bridge = Bridge::new(0, "Fixture".into(), sender);
    let mut requests = Requests::default();
    let activation = wait_event(&mut loop_, &mut requests, |event| {
        matches!(event, Event::Activate(_))
    })?;
    assert!(matches!(activation, Event::Activate(0)));
    let mut tree = tree();
    bridge.activate();
    bridge.publish(&tree, true);
    bridge.window_focus(true);
    let app = embeds.recv()?;
    let mut windows = wait_for(
        &mut signals,
        || children(&client, &app),
        |windows| !windows.is_empty(),
    )?;
    let window = windows.remove(0);
    let nodes = children(&client, &window)?;
    assert_eq!(nodes.len(), 2);
    check_button(
        &client,
        &nodes[0],
        &mut bridge,
        &mut loop_,
        &mut requests,
        &tree,
    )?;
    let text = &nodes[1];
    let text_proxy = proxy(&client, text, "org.a11y.atspi.Text")?;
    assert_eq!(
        text_proxy.call::<_, _, String>("GetText", &(0i32, -1i32))?,
        "a💡é"
    );
    assert_eq!(text_proxy.get_property::<i32>("CharacterCount")?, 3);
    let selection: bool = text_proxy.call("SetSelection", &(0i32, 1i32, 3i32))?;
    assert!(selection);
    let Event::Action(_, action) = wait_event(&mut loop_, &mut requests, |event| {
        matches!(event, Event::Action(0, _))
    })?
    else {
        return Err(TestError::UnexpectedEvent);
    };
    assert_eq!(
        bridge
            .action(action, &tree)
            .ok_or(TestError::RefusedAction)?
            .action,
        AccessibilityAction::SetTextSelection(TextSelection {
            anchor: 1,
            focus: 3
        })
    );
    text_navigation(&text_proxy, &mut signals, &mut bridge, &mut tree)?;
    lifecycle(&client, &mut loop_, &mut requests, &mut bridge, &tree)?;
    // Reactivation reconnects the accessibility bus and registers a fresh unique name.
    let app = embeds.recv()?;
    let _window = wait_for(
        &mut signals,
        || children(&client, &app),
        |windows| windows.len() == 1,
    )?;
    drop(bridge);
    let _no_window = wait_for(&mut signals, || children(&client, &app), Vec::is_empty)?;
    loop_.handle().remove(source);
    eprintln!(
        "AT-SPI private bus passed: name, state, click, Unicode selection, word/line navigation, wake, lifecycle"
    );
    Ok(())
}

/// Checks the name, the focus state and the click of the button.
fn check_button(
    client: &Connection,
    button: &Address,
    bridge: &mut Bridge,
    loop_: &mut EventLoop<Requests>,
    requests: &mut Requests,
    tree: &AccessibilityTree,
) -> Result<(), TestError> {
    let accessible = proxy(client, button, "org.a11y.atspi.Accessible")?;
    assert_eq!(accessible.get_property::<String>("Name")?, "Start");
    let inspect: Vec<u32> = accessible.call("GetState", &())?;
    assert_ne!(inspect[0] & (1 << 12), 0); // AT-SPI FOCUSED follows the keyboard focus of the host.
    bridge.window_focus(false);
    let inspect: Vec<u32> = accessible.call("GetState", &())?;
    assert_eq!(inspect[0] & (1 << 12), 0);
    bridge.window_focus(true);
    assert_eq!(tree.focus, Some(NodeId(1))); // Querying semantics never focuses another node.
    let accepted: bool =
        proxy(client, button, "org.a11y.atspi.Action")?.call("DoAction", &(0i32,))?;
    assert!(accepted);
    let Event::Action(_, action) = wait_event(loop_, requests, |event| {
        matches!(event, Event::Action(0, _))
    })?
    else {
        return Err(TestError::UnexpectedEvent);
    };
    // The bridge queues the click action before it replies to `DoAction`. An action caused by the earlier focus
    // changes would have been queued before the click, so the queue would contain it now.
    assert!(
        !requests
            .events
            .iter()
            .any(|event| matches!(event, Event::Action(_, _)))
    );
    assert_eq!(
        bridge
            .action(action, tree)
            .ok_or(TestError::RefusedAction)?,
        ActionRequest {
            target: NodeId(1),
            action: AccessibilityAction::Activate,
        }
    );
    Ok(())
}

fn text_navigation(
    proxy: &Proxy<'_>,
    signals: &mut MessageIterator,
    bridge: &mut Bridge,
    tree: &mut AccessibilityTree,
) -> Result<(), TestError> {
    for (value, runs) in [
        ("one two three", Vec::new()),
        (
            "abcאבג\nnext",
            vec![
                super::text::run(0, "abc", TextDirection::LeftToRight, 0.dp())?,
                super::text::run(3, "אבג\n", TextDirection::RightToLeft, 0.dp())?,
                super::text::run(7, "next", TextDirection::LeftToRight, 20.dp())?,
            ],
        ),
    ] {
        node_mut(tree, NodeId(1 << 63))?.text = Some(AccessibleText {
            value: value.into(),
            runs,
            ..Default::default()
        });
        tree.revision += 1;
        bridge.publish(tree, false);
        let _text = wait_for(
            signals,
            || Ok(proxy.call::<_, _, String>("GetText", &(0i32, -1i32))?),
            |text| text == value,
        )?;
        if value == "one two three" {
            let word: (String, i32, i32) = proxy.call("GetStringAtOffset", &(4i32, 1u32))?;
            assert_eq!(word, ("two ".into(), 4, 8));
        } else {
            let line: (String, i32, i32) = proxy.call("GetStringAtOffset", &(4i32, 3u32))?;
            assert_eq!(line, ("abcאבג\n".into(), 0, 7));
            let line: (String, i32, i32) = proxy.call("GetStringAtOffset", &(7i32, 3u32))?;
            assert_eq!(line, ("next".into(), 7, 11));
        }
    }
    Ok(())
}

fn lifecycle(
    client: &Connection,
    loop_: &mut EventLoop<Requests>,
    requests: &mut Requests,
    bridge: &mut Bridge,
    tree: &AccessibilityTree,
) -> Result<(), TestError> {
    let status = Proxy::new(client, "org.a11y.Bus", "/org/a11y/bus", "org.a11y.Status")?;
    status.set_property("IsEnabled", false)?;
    let deactivation = wait_event(loop_, requests, |event| {
        matches!(event, Event::Deactivate(_))
    })?;
    assert!(matches!(deactivation, Event::Deactivate(0)));
    bridge.deactivate();
    status.set_property("IsEnabled", true)?;
    let activation = wait_event(loop_, requests, |event| matches!(event, Event::Activate(_)))?;
    assert!(matches!(activation, Event::Activate(0)));
    bridge.activate();
    bridge.publish(tree, true);
    Ok(())
}
