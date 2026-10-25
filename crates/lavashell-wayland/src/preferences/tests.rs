// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::Error;
use crate::preference_reader::PreferenceReader;
use crate::test_error::TestError;
use lavashell_core::preferences::{PreferenceOverrides, PreferenceSource};
use smithay_client_toolkit::reexports::calloop::{EventLoop, channel};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use zbus::blocking::{Connection, connection::Builder};
use zbus::zvariant::OwnedValue;

#[test]
fn portal_values_use_only_standard_namespace_keys_and_known_u32_preferences() {
    let mut values = Appearance::default();
    portal::apply(
        &mut values,
        "other.namespace",
        "reduced-motion",
        &OwnedValue::from(1u32),
    );
    assert!(!values.reduced_motion);
    portal::apply(
        &mut values,
        "org.freedesktop.appearance",
        "reduced-motion",
        &OwnedValue::from(1u32),
    );
    assert!(values.reduced_motion);
    portal::apply(
        &mut values,
        "org.freedesktop.appearance",
        "contrast",
        &OwnedValue::from(1u32),
    );
    assert_eq!(values.contrast, ContrastPreference::More);
    portal::apply(
        &mut values,
        "org.freedesktop.appearance",
        "contrast",
        &OwnedValue::from(42u32),
    );
    assert_eq!(values.contrast, ContrastPreference::NoPreference);
    portal::apply(
        &mut values,
        "org.freedesktop.appearance",
        "reduced-motion",
        &OwnedValue::from(1i32),
    );
    assert!(!values.reduced_motion);
    for (preference, scheme) in [
        (1u32, Some(ColorScheme::Dark)),
        (2, Some(ColorScheme::Light)),
        (0, None),
    ] {
        portal::apply(
            &mut values,
            "org.freedesktop.appearance",
            "color-scheme",
            &OwnedValue::from(preference),
        );
        assert_eq!(values.color_scheme, scheme);
    }
}

struct Settings;

#[zbus::interface(name = "org.freedesktop.portal.Settings")]
impl Settings {
    async fn read_all(
        &self,
        namespaces: Vec<String>,
        #[zbus(connection)] connection: &zbus::Connection,
    ) -> zbus::fdo::Result<HashMap<String, HashMap<String, OwnedValue>>> {
        assert_eq!(namespaces, ["org.freedesktop.appearance"]);
        // The mock portal emits a change before it replies. The watch has to skip that change, because the
        // `ReadAll` reply is newer.
        connection
            .emit_signal(
                None::<&str>,
                "/org/freedesktop/portal/desktop",
                "org.freedesktop.portal.Settings",
                "SettingChanged",
                &(
                    "org.freedesktop.appearance",
                    "contrast",
                    OwnedValue::from(0u32),
                ),
            )
            .await?;
        Ok(HashMap::from([(
            "org.freedesktop.appearance".into(),
            HashMap::from([
                ("contrast".into(), OwnedValue::from(1u32)),
                ("reduced-motion".into(), OwnedValue::from(0u32)),
            ]),
        )]))
    }
}

#[test]
fn private_bus_portal_roundtrip() -> Result<(), TestError> {
    crate::test_bus::run(
        "LAVASHELL_PRIVATE_PORTAL_CHILD",
        "preferences::tests::private_bus_portal_roundtrip",
        child,
    )
}

/// The updates that the portal watch sent to the event loop, and its failure once it ended.
#[derive(Default)]
struct Updates {
    appearances: Vec<Appearance>,
    ended: Option<TestError>,
}

/// Dispatches the event loop until `count` updates arrived. Fails when the watch ended before that.
fn dispatch_until(
    loop_: &mut EventLoop<Updates>,
    updates: &mut Updates,
    count: usize,
) -> Result<(), TestError> {
    while updates.appearances.len() < count {
        if let Some(ended) = updates.ended.take() {
            return Err(ended);
        }
        loop_.dispatch(None, updates)?;
    }
    Ok(())
}

fn signal(
    connection: &Connection,
    namespace: &str,
    key: &str,
    value: u32,
) -> Result<(), TestError> {
    connection.emit_signal(
        None::<&str>,
        "/org/freedesktop/portal/desktop",
        "org.freedesktop.portal.Settings",
        "SettingChanged",
        &(namespace, key, OwnedValue::from(value)),
    )?;
    Ok(())
}

/// Watches the portal on the bus at `address`.
async fn watch_bus(address: String, events: channel::Sender<Appearance>) -> zbus::Result<()> {
    let connection = zbus::connection::Builder::address(address.as_str())?
        .build()
        .await?;
    portal::watch_connection(&connection, events).await
}

fn child() -> Result<(), TestError> {
    let address = std::env::var("DBUS_SESSION_BUS_ADDRESS")?;
    let service = Builder::address(address.as_str())?
        .name("org.freedesktop.portal.Desktop")?
        .serve_at("/org/freedesktop/portal/desktop", Settings)?
        .build()?;
    let mut source = PreferenceSource::default();
    source.set_overrides(PreferenceOverrides {
        reduced_motion: Some(false),
        ..PreferenceOverrides::default()
    });
    let reader = Rc::new(RefCell::new(PreferenceReader::new(
        lavashell_test_platform::facts(),
    )));
    reader.borrow().view.provide_preferences(&source.values());
    let source = Rc::new(RefCell::new(source));
    let mut loop_: EventLoop<Updates> = EventLoop::try_new()?;
    let (sender, receiver) = channel::channel::<Appearance>();
    let (handle, view) = (source.clone(), reader.clone());
    let receipt = loop_
        .handle()
        .insert_source(receiver, move |event, (), updates| {
            let channel::Event::Msg(update) = event else {
                // A failed watch also closes the channel. The error already stored for that failure by the
                // executor callback is not replaced.
                if updates.ended.is_none() {
                    updates.ended = Some(TestError::UpdatesClosed);
                }
                return;
            };
            let mut source = handle.borrow_mut();
            let system = update.applied_to(source.system());
            source.set_system(system);
            view.borrow().view.provide_preferences(&source.values());
            updates.appearances.push(update);
        })?;
    let (executor, scheduler) = calloop::futures::executor::<zbus::Result<()>>()?;
    let watch = loop_
        .handle()
        .insert_source(executor, |watched, (), updates| {
            let ended = watched.map_or_else(TestError::WatchFailed, |()| TestError::WatchEnded);
            updates.ended = Some(ended);
        })?;
    scheduler
        .schedule(watch_bus(address, sender))
        .map_err(Error::SchedulePreferenceWatch)?;
    let mut updates = Updates::default();
    dispatch_until(&mut loop_, &mut updates, 1)?;
    assert_eq!(
        reader.borrow_mut().values().contrast,
        ContrastPreference::More
    );
    signal(&service, "org.freedesktop.appearance", "reduced-motion", 1)?;
    dispatch_until(&mut loop_, &mut updates, 2)?;
    assert!(source.borrow().system().reduced_motion);
    assert!(!reader.borrow_mut().values().reduced_motion);
    signal(&service, "org.freedesktop.appearance", "contrast", 42)?;
    dispatch_until(&mut loop_, &mut updates, 3)?;
    assert_eq!(
        reader.borrow_mut().values().contrast,
        ContrastPreference::NoPreference
    );
    // The bus delivers signals in the order they were sent. A wrongly applied signal of the other namespace
    // would therefore arrive as the fourth update, before the reduced-motion change.
    signal(&service, "other.namespace", "contrast", 1)?;
    signal(&service, "org.freedesktop.appearance", "reduced-motion", 0)?;
    dispatch_until(&mut loop_, &mut updates, 4)?;
    assert_eq!(
        updates.appearances[3].contrast,
        ContrastPreference::NoPreference
    );
    assert!(!updates.appearances[3].reduced_motion);
    // Removing the executor source from the event loop drops the watch future and ends the watch.
    loop_.handle().remove(watch);
    loop_.handle().remove(receipt);
    // Once the mock portal is dropped, the private bus has no Settings portal. `portal::watch` then fails with the
    // error returned by the bus.
    drop(service);
    let (sender, _receiver) = channel::channel();
    assert!(futures_lite::future::block_on(portal::watch(sender)).is_err());
    eprintln!(
        "Portal private bus passed: initial read, changed signal, idle wake, override, unknown value, stop, fallback"
    );
    Ok(())
}
