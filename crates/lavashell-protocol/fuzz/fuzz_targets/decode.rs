// SPDX-License-Identifier: GPL-3.0-only
//! Decodes the bytes as each message of either direction and checks the memory that the decode
//! charges to the account: nothing when it fails, and exactly what the value needs when it
//! succeeds, so one byte less fails and a release gives all of it back.
#![forbid(unsafe_code)]
#![no_main]

use lavashell_protocol::{Account, GuestMessage, Hello, HostMessage, ProtocolError, decode};
use libfuzzer_sys::fuzz_target;
use serde::de::DeserializeOwned;

fuzz_target!(|bytes: &[u8]| {
    decode_within_account::<Hello>(bytes);
    decode_within_account::<HostMessage>(bytes);
    decode_within_account::<GuestMessage>(bytes);
});

fn decode_within_account<T: DeserializeOwned>(bytes: &[u8]) {
    let mut unlimited = Account::new(usize::MAX);
    let Ok(decoded) = decode::<T>(bytes, &mut unlimited) else {
        assert_eq!(unlimited.remaining(), usize::MAX);
        return;
    };
    let needed = decoded.charge.bytes();
    assert_eq!(unlimited.remaining(), usize::MAX - needed);
    unlimited.release(decoded.charge);
    assert_eq!(unlimited.remaining(), usize::MAX);
    let mut exact = Account::new(needed);
    assert!(decode::<T>(bytes, &mut exact).is_ok());
    assert_eq!(exact.remaining(), 0);
    if let Some(short) = needed.checked_sub(1) {
        let mut account = Account::new(short);
        let insufficient = decode::<T>(bytes, &mut account);
        assert!(matches!(insufficient, Err(ProtocolError::Charge { .. })));
        assert_eq!(account.remaining(), short);
    }
}
