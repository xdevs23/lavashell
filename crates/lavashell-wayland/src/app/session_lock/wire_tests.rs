// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::app::wire_messages::{FakeCompositor, WireMessage};
use crate::test_error::TestError;
use sctk::reexports::client::protocol::{wl_display, wl_registry};
use sctk::reexports::client::{Proxy, delegate_noop};
use sctk::reexports::protocols::ext::session_lock::v1::client::{
    ext_session_lock_manager_v1, ext_session_lock_v1,
};

struct Registry;

delegate_noop!(Registry: ignore wl_registry::WlRegistry);

#[test]
fn an_unlock_is_sent_before_the_sync_that_confirms_it() -> Result<(), TestError> {
    let mut compositor = FakeCompositor::new()?;
    let qh = compositor
        .connection
        .new_event_queue::<Translator>()
        .handle();
    let registry_qh = compositor.connection.new_event_queue::<Registry>().handle();
    let registry = compositor
        .connection
        .display()
        .get_registry(&registry_qh, ());
    let manager = registry.bind::<ExtSessionLockManagerV1, _, _>(1, 1, &qh, ());
    assert!(!compositor.flush()?.is_empty());
    let lock = request_lock(&manager, &qh);
    let manager = manager.id().protocol_id();
    let proxy = lock.id().protocol_id();
    assert_eq!(
        compositor.flush()?,
        vec![
            WireMessage::new(
                manager,
                ext_session_lock_manager_v1::REQ_LOCK_OPCODE,
                &[proxy]
            ),
            WireMessage::new(
                manager,
                ext_session_lock_manager_v1::REQ_DESTROY_OPCODE,
                &[]
            ),
        ]
    );
    send_unlock(&lock, &compositor.connection, &qh);
    let sent = compositor.flush()?;
    let display = compositor.connection.display().id().protocol_id();
    let callback = sent[1].arguments[0];
    assert_eq!(
        sent,
        vec![
            WireMessage::new(
                proxy,
                ext_session_lock_v1::REQ_UNLOCK_AND_DESTROY_OPCODE,
                &[]
            ),
            WireMessage::new(display, wl_display::REQ_SYNC_OPCODE, &[callback]),
        ]
    );
    Ok(())
}
