// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::test_error::TestError;
use crate::test_portal::{TestSettingChange, TestSettingsPortal, emit_setting_changed};
use futures_lite::future::block_on;
use lavashell_test_fixtures::test_bus;
use zbus::connection::Builder;

const NAMESPACE: &str = "test.namespace";

fn serve(
    read_all_reply: zbus::fdo::Result<HashMap<String, HashMap<String, OwnedValue>>>,
    changed_before_reply: Vec<TestSettingChange>,
) -> Result<Connection, TestError> {
    Ok(block_on(
        TestSettingsPortal {
            read_all_reply,
            changed_before_reply,
        }
        .serve(),
    )?)
}

fn watch() -> Result<WatchedNamespace, TestError> {
    block_on(watch_namespace(NAMESPACE))?.ok_or(TestError::PortalMissing)
}

fn change(key: &'static str) -> TestSettingChange {
    TestSettingChange {
        namespace: NAMESPACE,
        key,
        value: 1,
    }
}

struct PortalWithoutSettings;

#[zbus::interface(name = "org.freedesktop.portal.Other")]
impl PortalWithoutSettings {}

#[test]
fn a_session_bus_without_the_settings_portal_leaves_the_namespace_unwatched()
-> Result<(), TestError> {
    test_bus::run(&["DBUS_SESSION_BUS_ADDRESS"], || {
        assert!(block_on(watch_namespace(NAMESPACE))?.is_none());
        Ok(())
    })
}

#[test]
fn a_portal_without_the_settings_interface_leaves_the_namespace_unwatched() -> Result<(), TestError>
{
    test_bus::run(&["DBUS_SESSION_BUS_ADDRESS"], || {
        let _portal_served_for_the_test = block_on(
            Builder::session()?
                .name(DESTINATION)?
                .serve_at(PATH, PortalWithoutSettings)?
                .build(),
        )?;
        assert!(block_on(watch_namespace(NAMESPACE))?.is_none());
        Ok(())
    })
}

#[test]
fn a_settings_portal_that_fails_to_read_fails() -> Result<(), TestError> {
    test_bus::run(&["DBUS_SESSION_BUS_ADDRESS"], || {
        let _portal_served_for_the_test = serve(
            Err(zbus::fdo::Error::Failed("the backend stopped".to_owned())),
            Vec::new(),
        )?;
        assert!(matches!(
            block_on(watch_namespace(NAMESPACE)),
            Err(PortalError::ReadSettings(_))
        ));
        Ok(())
    })
}

#[test]
fn a_namespace_missing_from_the_reply_has_no_values() -> Result<(), TestError> {
    test_bus::run(&["DBUS_SESSION_BUS_ADDRESS"], || {
        let _service_stays_on_the_bus_for_the_test = serve(Ok(HashMap::new()), Vec::new())?;
        assert!(watch()?.values.is_empty());
        Ok(())
    })
}

#[test]
fn a_change_sent_before_the_reply_is_skipped() -> Result<(), TestError> {
    test_bus::run(&["DBUS_SESSION_BUS_ADDRESS"], || {
        let service = serve(Ok(HashMap::new()), vec![change("before-reply")])?;
        let mut changes = watch()?.changes;
        block_on(emit_setting_changed(&service, &change("after-reply")))?;
        let next = block_on(changes.next_change()).ok_or(TestError::ChangesEnded)??;
        assert_eq!(next.key, "after-reply");
        Ok(())
    })
}

#[test]
fn a_change_with_a_malformed_body_fails() -> Result<(), TestError> {
    test_bus::run(&["DBUS_SESSION_BUS_ADDRESS"], || {
        let service = serve(Ok(HashMap::new()), Vec::new())?;
        let mut changes = watch()?.changes;
        block_on(service.emit_signal(
            None::<&str>,
            PATH,
            INTERFACE,
            "SettingChanged",
            &(NAMESPACE, 1u32),
        ))?;
        assert!(matches!(
            block_on(changes.next_change()),
            Some(Err(PortalError::ReadSettingChange(_)))
        ));
        Ok(())
    })
}
