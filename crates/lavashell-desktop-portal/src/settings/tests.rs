// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::test_error::TestError;
use crate::test_portal::{TestSettingChange, TestSettingsPortal, emit_setting_changed};
use futures_lite::future::block_on;
use lavashell_test_fixtures::test_bus;

const NAMESPACE: &str = "test.namespace";

fn serve(changed_before_reply: Vec<TestSettingChange>) -> Result<Connection, TestError> {
    Ok(block_on(
        TestSettingsPortal {
            namespaces: HashMap::new(),
            changed_before_reply,
        }
        .serve(),
    )?)
}

fn change(key: &'static str) -> TestSettingChange {
    TestSettingChange {
        namespace: NAMESPACE,
        key,
        value: 1,
    }
}

#[test]
fn a_namespace_missing_from_the_reply_has_no_values() -> Result<(), TestError> {
    test_bus::run(&["DBUS_SESSION_BUS_ADDRESS"], || {
        let _service_stays_on_the_bus_for_the_test = serve(Vec::new())?;
        assert!(block_on(watch_namespace(NAMESPACE))?.values.is_empty());
        Ok(())
    })
}

#[test]
fn a_change_sent_before_the_reply_is_skipped() -> Result<(), TestError> {
    test_bus::run(&["DBUS_SESSION_BUS_ADDRESS"], || {
        let service = serve(vec![change("before-reply")])?;
        let mut changes = block_on(watch_namespace(NAMESPACE))?.changes;
        block_on(emit_setting_changed(&service, &change("after-reply")))?;
        let next = block_on(changes.next_change()).ok_or(TestError::ChangesEnded)??;
        assert_eq!(next.key, "after-reply");
        Ok(())
    })
}

#[test]
fn a_change_with_a_malformed_body_fails() -> Result<(), TestError> {
    test_bus::run(&["DBUS_SESSION_BUS_ADDRESS"], || {
        let service = serve(Vec::new())?;
        let mut changes = block_on(watch_namespace(NAMESPACE))?.changes;
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
