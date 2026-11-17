// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use crate::accessibility::test_error::AccessibilityTestError;
use accesskit::{ActionData, ActionRequest, TextPosition, TextSelection};
use atspi_common::{State, StateSet};
use futures_lite::StreamExt;
use futures_lite::future::block_on;
use lavashell_actor::{Closed, Receiver};
use serde::{Deserialize, Serialize};
use std::sync::mpsc;
use zbus::zvariant::{OwnedObjectPath, Type};
use zbus::{Connection, MessageStream, Proxy, connection::Builder};

const SURFACE_0: SurfaceId = SurfaceId::numbered(0);

#[derive(Deserialize, Serialize, Type)]
#[zvariant(crate = "zbus::zvariant")]
struct AtspiAddress {
    bus_name: String,
    path: OwnedObjectPath,
}

struct Reported {
    view: Receiver<PlatformEvent>,
    events: Vec<AccessibilityEvent>,
}

struct Status {
    enabled: bool,
}

#[zbus::interface(name = "org.a11y.Status")]
impl Status {
    #[zbus(property)]
    fn is_enabled(&self) -> bool {
        self.enabled
    }

    #[zbus(property)]
    fn set_is_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    #[zbus(property)]
    fn screen_reader_enabled(&self) -> bool {
        self.enabled
    }
}

const ROOT: &str = "/org/a11y/atspi/accessible/root";

struct Registry {
    embedded: mpsc::Sender<AtspiAddress>,
    name: String,
    root: OwnedObjectPath,
}

#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "org.lavashell.test")]
enum RegistryError {
    #[zbus(error)]
    Bus(zbus::Error),
    EmbedsReceiverGone,
}

#[zbus::interface(name = "org.a11y.atspi.Socket")]
impl Registry {
    fn embed(&self, plug: AtspiAddress) -> Result<(AtspiAddress,), RegistryError> {
        if self.embedded.send(plug).is_err() {
            return Err(RegistryError::EmbedsReceiverGone);
        }
        Ok((AtspiAddress {
            bus_name: self.name.clone(),
            path: self.root.clone(),
        },))
    }
}

#[test]
fn atspi_clients_read_the_tree_and_send_actions_through_the_bridge() -> Result<(), TestError> {
    crate::test_bus::run(&["DBUS_SESSION_BUS_ADDRESS", "AT_SPI_BUS_ADDRESS"], || {
        block_on(child())
    })
}

async fn proxy<'a>(
    connection: &'a Connection,
    address: &'a AtspiAddress,
    interface: &'a str,
) -> Result<Proxy<'a>, TestError> {
    Ok(Proxy::new(
        connection,
        address.bus_name.as_str(),
        address.path.as_str(),
        interface,
    )
    .await?)
}

fn wait_event(
    reported: &mut Reported,
    predicate: impl Fn(&AccessibilityEvent) -> bool,
) -> Result<AccessibilityEvent, TestError> {
    loop {
        if let Some(index) = reported.events.iter().position(&predicate) {
            return Ok(reported.events.remove(index));
        }
        let arrived = reported
            .view
            .take()
            .map_err(|Closed| AccessibilityTestError::EventsClosed)?;
        reported.events.extend(accessibility_events(arrived));
    }
}

async fn signals(client: &Connection) -> Result<MessageStream, TestError> {
    let rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .build();
    Ok(MessageStream::for_match_rule(rule, client, None).await?)
}

async fn wait_for<T>(
    signals: &mut MessageStream,
    mut read: impl AsyncFnMut() -> Result<T, TestError>,
    holds: impl Fn(&T) -> bool,
) -> Result<T, TestError> {
    loop {
        let value = read().await?;
        if holds(&value) {
            return Ok(value);
        }
        let signal = signals
            .next()
            .await
            .ok_or(AccessibilityTestError::BusClosed)?;
        assert_eq!(signal?.message_type(), zbus::message::Type::Signal);
    }
}

async fn children(
    connection: &Connection,
    address: &AtspiAddress,
) -> Result<Vec<AtspiAddress>, TestError> {
    Ok(proxy(connection, address, "org.a11y.atspi.Accessible")
        .await?
        .call("GetChildren", &())
        .await?)
}

async fn serve_fixture(
    address: &str,
) -> Result<(Connection, Connection, mpsc::Receiver<AtspiAddress>), TestError> {
    let (embedded, embeds) = mpsc::channel();
    let status = Builder::address(address)?
        .name("org.a11y.Bus")?
        .serve_at("/org/a11y/bus", Status { enabled: true })?
        .build()
        .await?;
    let registry = Builder::address(address)?
        .name("org.a11y.atspi.Registry")?
        .build()
        .await?;
    let name = registry
        .unique_name()
        .ok_or(AccessibilityTestError::NoUniqueName)?
        .to_string();
    let served = registry
        .object_server()
        .at(
            ROOT,
            Registry {
                embedded,
                name,
                root: OwnedObjectPath::try_from(ROOT)?,
            },
        )
        .await?;
    assert!(served);
    Ok((status, registry, embeds))
}

async fn child() -> Result<(), TestError> {
    let address = std::env::var("DBUS_SESSION_BUS_ADDRESS")?;
    let (_status, _registry, embeds) = serve_fixture(&address).await?;
    let client = Builder::address(address.as_str())?.build().await?;
    let mut signals = signals(&client).await?;
    let (sender, view) = mailbox::<PlatformEvent>();
    let mut bridge = Bridge::new(
        SurfaceId::numbered(0),
        ViewLink::new(sender.outlet(|event| event)),
    );
    drop(sender);
    let mut reported = Reported {
        view,
        events: Vec::new(),
    };
    let activation = wait_event(&mut reported, |event| {
        matches!(event, AccessibilityEvent::Activate(_))
    })?;
    assert!(matches!(
        activation,
        AccessibilityEvent::Activate(SURFACE_0)
    ));
    let initial = tree(&[Run::line("a💡é", &[0])])?;
    bridge.publish(initial.clone());
    bridge.window_focus(true);
    let app = embeds.recv()?;
    let mut windows = wait_for(
        &mut signals,
        || children(&client, &app),
        |windows| !windows.is_empty(),
    )
    .await?;
    let window = windows.remove(0);
    let nodes = children(&client, &window).await?;
    assert_eq!(nodes.len(), 2);
    check_button(&client, &nodes[0], &mut bridge, &mut reported).await?;
    let text = &nodes[1];
    let text_proxy = proxy(&client, text, "org.a11y.atspi.Text").await?;
    assert_eq!(
        text_proxy
            .call::<_, _, String>("GetText", &(0i32, -1i32))
            .await?,
        "a💡é"
    );
    assert_eq!(text_proxy.get_property::<i32>("CharacterCount").await?, 3);
    let selection: bool = text_proxy.call("SetSelection", &(0i32, 1i32, 3i32)).await?;
    assert!(selection);
    let AccessibilityEvent::Action(SURFACE_0, action) = wait_event(&mut reported, |event| {
        matches!(event, AccessibilityEvent::Action(SURFACE_0, _))
    })?
    else {
        return Err(AccessibilityTestError::UnexpectedEvent.into());
    };
    let run = |character_index| TextPosition {
        node: NodeId(FIRST_RUN),
        character_index,
    };
    let selection = TextSelection {
        anchor: run(1),
        focus: run(3),
    };
    assert_eq!(
        action,
        ActionRequest {
            action: Action::SetTextSelection,
            target_tree: TreeId::ROOT,
            target_node: FIELD,
            data: Some(ActionData::SetTextSelection(selection)),
        }
    );
    text_navigation(&text_proxy, &mut signals, &mut bridge).await?;
    lifecycle(&client, &mut reported, &mut bridge, initial).await?;
    let app = embeds.recv()?;
    let _shown_window_is_not_checked = wait_for(
        &mut signals,
        || children(&client, &app),
        |windows| windows.len() == 1,
    )
    .await?;
    drop(bridge);
    let _empty_children_are_not_checked =
        wait_for(&mut signals, || children(&client, &app), Vec::is_empty).await?;
    Ok(())
}

async fn check_button(
    client: &Connection,
    button: &AtspiAddress,
    bridge: &mut Bridge,
    reported: &mut Reported,
) -> Result<(), TestError> {
    let accessible = proxy(client, button, "org.a11y.atspi.Accessible").await?;
    assert_eq!(accessible.get_property::<String>("Name").await?, "Start");
    let states: StateSet = accessible.call("GetState", &()).await?;
    assert!(states.contains(State::Focused));
    bridge.window_focus(false);
    let states: StateSet = accessible.call("GetState", &()).await?;
    assert!(!states.contains(State::Focused));
    bridge.window_focus(true);
    let accepted: bool = proxy(client, button, "org.a11y.atspi.Action")
        .await?
        .call("DoAction", &(0i32,))
        .await?;
    assert!(accepted);
    let AccessibilityEvent::Action(SURFACE_0, action) = wait_event(reported, |event| {
        matches!(event, AccessibilityEvent::Action(SURFACE_0, _))
    })?
    else {
        return Err(AccessibilityTestError::UnexpectedEvent.into());
    };
    assert!(
        !reported
            .events
            .iter()
            .any(|event| matches!(event, AccessibilityEvent::Action(_, _)))
    );
    assert_eq!(
        action,
        ActionRequest {
            action: Action::Click,
            target_tree: TreeId::ROOT,
            target_node: BUTTON,
            data: None,
        }
    );
    Ok(())
}

async fn text_navigation(
    proxy: &Proxy<'_>,
    signals: &mut MessageStream,
    bridge: &mut Bridge,
) -> Result<(), TestError> {
    let words = [Run::line("one two three", &[0, 4, 8])];
    let lines = [
        Run {
            continues_line: true,
            ..Run::line("abc", &[0])
        },
        Run {
            direction: TextDirection::RightToLeft,
            ..Run::line("אבג\n", &[])
        },
        Run::line("next", &[0]),
    ];
    for (value, runs) in [("one two three", &words[..]), ("abcאבג\nnext", &lines[..])] {
        bridge.publish(tree(runs)?);
        let _published_text_is_not_checked = wait_for(
            signals,
            async || {
                Ok(proxy
                    .call::<_, _, String>("GetText", &(0i32, -1i32))
                    .await?)
            },
            |text| text == value,
        )
        .await?;
        if value == "one two three" {
            let word: (String, i32, i32) = proxy.call("GetStringAtOffset", &(4i32, 1u32)).await?;
            assert_eq!(word, ("two ".into(), 4, 8));
        } else {
            let line: (String, i32, i32) = proxy.call("GetStringAtOffset", &(4i32, 3u32)).await?;
            assert_eq!(line, ("abcאבג\n".into(), 0, 7));
            let line: (String, i32, i32) = proxy.call("GetStringAtOffset", &(7i32, 3u32)).await?;
            assert_eq!(line, ("next".into(), 7, 11));
        }
    }
    Ok(())
}

async fn lifecycle(
    client: &Connection,
    reported: &mut Reported,
    bridge: &mut Bridge,
    initial: TreeUpdate,
) -> Result<(), TestError> {
    let status = Proxy::new(client, "org.a11y.Bus", "/org/a11y/bus", "org.a11y.Status").await?;
    status.set_property("IsEnabled", false).await?;
    let deactivation = wait_event(reported, |event| {
        matches!(event, AccessibilityEvent::Deactivate(_))
    })?;
    assert!(matches!(
        deactivation,
        AccessibilityEvent::Deactivate(SURFACE_0)
    ));
    status.set_property("IsEnabled", true).await?;
    let activation = wait_event(reported, |event| {
        matches!(event, AccessibilityEvent::Activate(_))
    })?;
    assert!(matches!(
        activation,
        AccessibilityEvent::Activate(SURFACE_0)
    ));
    bridge.publish(initial);
    Ok(())
}
