// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::test_error::TestError;
use calloop::{EventLoop, channel};
use futures_lite::future::block_on;
use lavashell_desktop_portal::test_portal::{
    TestSettingChange, TestSettingsPortal, emit_setting_changed,
};
use lavashell_test_fixtures::test_bus;
use std::collections::HashMap;
use zbus::Connection;
use zbus::zvariant::OwnedValue;

fn appearance_of(setting: PortalKey, preference: u32) -> Appearance {
    setting.read(Some(preference), UNSET_APPEARANCE)
}

#[test]
fn portal_preferences_map_to_the_appearance() {
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
fn appearance_follows_the_portal_settings_on_the_session_bus() -> Result<(), TestError> {
    test_bus::run(&["DBUS_SESSION_BUS_ADDRESS"], child)
}

#[derive(Default)]
struct Updates {
    events: Vec<DesktopSetting>,
    ended: Option<TestError>,
}

fn dispatch_until(
    loop_: &mut EventLoop<Updates>,
    updates: &mut Updates,
    count: usize,
) -> Result<(), TestError> {
    while updates.events.len() < count {
        if let Some(ended) = updates.ended.take() {
            return Err(ended);
        }
        loop_.dispatch(None, updates)?;
    }
    Ok(())
}

fn signal(
    connection: &Connection,
    namespace: &'static str,
    key: &'static str,
    value: u32,
) -> Result<(), TestError> {
    block_on(emit_setting_changed(
        connection,
        &TestSettingChange {
            namespace,
            key,
            value,
        },
    ))?;
    Ok(())
}

fn build_appearance_setting(reduced_motion: bool, contrast: ContrastPreference) -> DesktopSetting {
    DesktopSetting::Appearance(Appearance {
        reduced_motion: Some(reduced_motion),
        contrast: Some(contrast),
        color_scheme: None,
    })
}

fn child() -> Result<(), TestError> {
    let service = block_on(
        TestSettingsPortal {
            namespaces: HashMap::from([(
                NAMESPACE.into(),
                HashMap::from([
                    ("contrast".into(), OwnedValue::from(1u32)),
                    ("reduced-motion".into(), OwnedValue::from(0u32)),
                    ("color-scheme".into(), OwnedValue::from(true)),
                ]),
            )]),
            changed_before_reply: vec![TestSettingChange {
                namespace: NAMESPACE,
                key: "contrast",
                value: 0,
            }],
        }
        .serve(),
    )?;
    let mut loop_: EventLoop<Updates> = EventLoop::try_new()?;
    let (sender, receiver) = channel::channel::<DesktopSetting>();
    let receipt = loop_
        .handle()
        .insert_source(receiver, |event, (), updates| {
            let channel::Event::Msg(update) = event else {
                if updates.ended.is_none() {
                    updates.ended = Some(TestError::UpdatesClosed);
                }
                return;
            };
            updates.events.push(update);
        })?;
    let (executor, scheduler) = calloop::futures::executor::<()>()?;
    let watch = loop_.handle().insert_source(executor, |(), (), updates| {
        updates.ended = Some(TestError::WatchEnded);
    })?;
    let report: SettingsReporter = Box::new(move |event| {
        let _ended_test_stops_reading_updates = sender.send(event);
    });
    scheduler
        .schedule(Arc::new(AppearanceSource).watch(report))
        .map_err(TestError::ScheduleWatch)?;
    let mut updates = Updates::default();
    dispatch_until(&mut loop_, &mut updates, 1)?;
    assert_eq!(
        updates.events[0],
        build_appearance_setting(false, ContrastPreference::More)
    );
    signal(&service, NAMESPACE, "reduced-motion", 1)?;
    dispatch_until(&mut loop_, &mut updates, 2)?;
    assert_eq!(
        updates.events[1],
        build_appearance_setting(true, ContrastPreference::More)
    );
    signal(&service, NAMESPACE, "contrast", 42)?;
    dispatch_until(&mut loop_, &mut updates, 3)?;
    assert_eq!(
        updates.events[2],
        build_appearance_setting(true, ContrastPreference::NoPreference)
    );
    signal(&service, "other.namespace", "contrast", 1)?;
    signal(&service, NAMESPACE, "reduced-motion", 0)?;
    dispatch_until(&mut loop_, &mut updates, 4)?;
    assert_eq!(
        updates.events[3],
        build_appearance_setting(false, ContrastPreference::NoPreference)
    );
    loop_.handle().remove(watch);
    loop_.handle().remove(receipt);
    drop(service);
    let ignore: SettingsReporter = Box::new(drop::<DesktopSetting>);
    assert!(matches!(
        block_on(watch_appearance(&ignore)),
        Err(PortalError::ReadSettings(_))
    ));
    Ok(())
}
