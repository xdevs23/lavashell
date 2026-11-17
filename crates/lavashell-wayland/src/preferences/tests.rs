// SPDX-License-Identifier: GPL-3.0-only
use super::test_error::PreferencesTestError;
use super::*;
use crate::Error;
use crate::PreferencesError;
use crate::test_error::TestError;
use futures_lite::future::block_on;
use lavashell_core::platform::ColorScheme;
use lavashell_core::preferences::{Appearance, ContrastPreference};
use portal::PortalKey;
use smithay_client_toolkit::reexports::calloop::{EventLoop, channel};
use std::collections::HashMap;
use zbus::zvariant::OwnedValue;
use zbus::{Connection, connection::Builder};

fn appearance_of(setting: PortalKey, preference: u32) -> Appearance {
    let mut settings = portal::AppearanceSettings::default();
    settings.set(setting, Some(preference));
    portal::appearance(settings)
}

#[test]
fn portal_preferences_map_to_the_appearance() {
    let unset = Appearance {
        reduced_motion: Some(false),
        contrast: Some(ContrastPreference::NoPreference),
        color_scheme: None,
    };
    assert_eq!(
        portal::appearance(portal::AppearanceSettings::default()),
        unset
    );
    for (preference, reduced_motion) in [(1, true), (0, false)] {
        assert_eq!(
            appearance_of(PortalKey::ReducedMotion, preference).reduced_motion,
            Some(reduced_motion)
        );
    }
    assert_eq!(
        appearance_of(PortalKey::Contrast, 1).contrast,
        Some(ContrastPreference::More)
    );
    assert_eq!(
        appearance_of(PortalKey::Contrast, 42).contrast,
        Some(ContrastPreference::NoPreference)
    );
    for (preference, scheme) in [
        (1u32, Some(ColorScheme::Dark)),
        (2, Some(ColorScheme::Light)),
        (0, None),
    ] {
        assert_eq!(
            appearance_of(PortalKey::ColorScheme, preference).color_scheme,
            scheme
        );
    }
}

#[test]
fn portal_keys_are_read_by_their_standard_names() {
    assert_eq!(
        "reduced-motion".parse::<PortalKey>().ok(),
        Some(PortalKey::ReducedMotion)
    );
    assert_eq!(
        "contrast".parse::<PortalKey>().ok(),
        Some(PortalKey::Contrast)
    );
    assert_eq!(
        "color-scheme".parse::<PortalKey>().ok(),
        Some(PortalKey::ColorScheme)
    );
    assert!("accent-color".parse::<PortalKey>().is_err());
}

struct Settings;

#[zbus::interface(name = "org.freedesktop.portal.Settings")]
impl Settings {
    async fn read_all(
        &self,
        namespaces: Vec<String>,
        #[zbus(connection)] connection: &Connection,
    ) -> zbus::fdo::Result<HashMap<String, HashMap<String, OwnedValue>>> {
        assert_eq!(namespaces, ["org.freedesktop.appearance"]);
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
                ("color-scheme".into(), OwnedValue::from(true)),
            ]),
        )]))
    }
}

#[test]
fn appearance_follows_the_portal_settings_on_the_session_bus() -> Result<(), TestError> {
    crate::test_bus::run(&["DBUS_SESSION_BUS_ADDRESS"], child)
}

#[derive(Default)]
struct Updates {
    appearances: Vec<Appearance>,
    ended: Option<PreferencesTestError>,
}

fn dispatch_until(
    loop_: &mut EventLoop<Updates>,
    updates: &mut Updates,
    count: usize,
) -> Result<(), TestError> {
    while updates.appearances.len() < count {
        if let Some(ended) = updates.ended.take() {
            return Err(ended.into());
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
    block_on(connection.emit_signal(
        None::<&str>,
        "/org/freedesktop/portal/desktop",
        "org.freedesktop.portal.Settings",
        "SettingChanged",
        &(namespace, key, OwnedValue::from(value)),
    ))?;
    Ok(())
}

async fn watch_bus(address: String, events: channel::Sender<Appearance>) -> zbus::Result<()> {
    let connection = Builder::address(address.as_str())?.build().await?;
    portal::watch_connection(&connection, events).await
}

fn child() -> Result<(), TestError> {
    let address = std::env::var("DBUS_SESSION_BUS_ADDRESS")?;
    let service = block_on(
        Builder::address(address.as_str())?
            .name("org.freedesktop.portal.Desktop")?
            .serve_at("/org/freedesktop/portal/desktop", Settings)?
            .build(),
    )?;
    let mut loop_: EventLoop<Updates> = EventLoop::try_new()?;
    let (sender, receiver) = channel::channel::<Appearance>();
    let receipt = loop_
        .handle()
        .insert_source(receiver, |event, (), updates| {
            let channel::Event::Msg(update) = event else {
                if updates.ended.is_none() {
                    updates.ended = Some(PreferencesTestError::UpdatesClosed);
                }
                return;
            };
            updates.appearances.push(update);
        })?;
    let (executor, scheduler) = calloop::futures::executor::<zbus::Result<()>>()?;
    let watch = loop_
        .handle()
        .insert_source(executor, |watched, (), updates| {
            let ended = watched.map_or_else(PreferencesTestError::WatchFailed, |()| {
                PreferencesTestError::WatchEnded
            });
            updates.ended = Some(ended);
        })?;
    scheduler
        .schedule(watch_bus(address, sender))
        .map_err(PreferencesError::SchedulePreferenceWatch)
        .map_err(Error::from)?;
    let mut updates = Updates::default();
    dispatch_until(&mut loop_, &mut updates, 1)?;
    assert_eq!(
        updates.appearances[0].contrast,
        Some(ContrastPreference::More)
    );
    assert_eq!(updates.appearances[0].color_scheme, None);
    signal(&service, "org.freedesktop.appearance", "reduced-motion", 1)?;
    dispatch_until(&mut loop_, &mut updates, 2)?;
    assert_eq!(updates.appearances[1].reduced_motion, Some(true));
    signal(&service, "org.freedesktop.appearance", "contrast", 42)?;
    dispatch_until(&mut loop_, &mut updates, 3)?;
    assert_eq!(
        updates.appearances[2].contrast,
        Some(ContrastPreference::NoPreference)
    );
    signal(&service, "other.namespace", "contrast", 1)?;
    signal(&service, "org.freedesktop.appearance", "reduced-motion", 0)?;
    dispatch_until(&mut loop_, &mut updates, 4)?;
    assert_eq!(
        updates.appearances[3].contrast,
        Some(ContrastPreference::NoPreference)
    );
    assert_eq!(updates.appearances[3].reduced_motion, Some(false));
    loop_.handle().remove(watch);
    loop_.handle().remove(receipt);
    drop(service);
    let (sender, _receiver) = channel::channel();
    assert!(block_on(portal::watch(sender)).is_err());
    Ok(())
}
