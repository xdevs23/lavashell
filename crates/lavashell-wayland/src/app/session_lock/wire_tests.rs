// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::app::wire_messages::messages;
use crate::test_error::TestError;
use sctk::reexports::client::Proxy;
use sctk::reexports::client::protocol::wl_registry;
use std::os::unix::net::UnixStream;

struct Registry;

delegate_noop!(Registry: ignore wl_registry::WlRegistry);

/// A session lock manager whose requests a test reads back from the other end of a socket.
struct Wire {
    connection: Connection,
    peer: UnixStream,
    qh: QueueHandle<Translator>,
    manager: ExtSessionLockManagerV1,
}

impl Wire {
    fn new() -> Result<Self, TestError> {
        let (socket, mut peer) = UnixStream::pair().map_err(TestError::CreateSocketPair)?;
        peer.set_nonblocking(true)
            .map_err(TestError::ConfigureSocket)?;
        let connection = Connection::from_socket(socket).map_err(Error::Connect)?;
        let qh = connection.new_event_queue::<Translator>().handle();
        let registry_qh = connection.new_event_queue::<Registry>().handle();
        let registry = connection.display().get_registry(&registry_qh, ());
        let manager = registry.bind::<ExtSessionLockManagerV1, _, _>(1, 1, &qh, ());
        connection.flush().map_err(Error::Flush)?;
        assert!(!messages(&mut peer)?.is_empty());
        Ok(Self {
            connection,
            peer,
            qh,
            manager,
        })
    }

    fn flush(&mut self) -> Result<Vec<(u32, u16, Vec<u32>)>, TestError> {
        self.connection.flush().map_err(Error::Flush)?;
        messages(&mut self.peer)
    }
}

#[test]
fn private_wire_an_unlock_is_sent_before_the_sync_that_confirms_it() -> Result<(), TestError> {
    let mut wire = Wire::new()?;
    let lock = request_lock(&wire.manager, &wire.qh);
    let manager = wire.manager.id().protocol_id();
    let proxy = lock.id().protocol_id();
    // Opcode 1 of ext_session_lock_manager_v1 is lock, and opcode 0 is destroy.
    assert_eq!(
        wire.flush()?,
        vec![(manager, 1, vec![proxy]), (manager, 0, vec![])]
    );
    send_unlock(&lock, &wire.connection, &wire.qh);
    let sent = wire.flush()?;
    let display = wire.connection.display().id().protocol_id();
    let callback = sent[1].2[0];
    // Opcode 2 of ext_session_lock_v1 is unlock_and_destroy, and opcode 0 of wl_display is sync.
    assert_eq!(sent, vec![(proxy, 2, vec![]), (display, 0, vec![callback])]);
    Ok(())
}
