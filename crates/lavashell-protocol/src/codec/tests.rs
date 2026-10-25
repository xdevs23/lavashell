// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::TestResult;
use lavashell_core::input_method::FieldText;
use std::collections::BTreeMap;

#[test]
fn decoding_charges_each_element_by_its_size_and_each_string_by_its_length() -> TestResult {
    let value = (
        vec![1_u64, 2, 3],
        vec![String::from("ab"), String::from("cde")],
    );
    let mut account = Account::new(1000);
    let read: Decoded<(Vec<u64>, Vec<String>)> = decode(encode(&value)?.bytes(), &mut account)?;
    assert_eq!(read.value, value);
    let charged = 3 * size_of::<u64>() + 2 * size_of::<String>() + "ab".len() + "cde".len();
    assert_eq!(read.charge.bytes(), charged);
    assert_eq!(account.remaining(), 1000 - charged);
    Ok(())
}

#[test]
fn decoding_charges_the_contents_of_options_and_variants() -> TestResult {
    type Containers = (Option<String>, Result<String, u8>);
    let value: Containers = (Some(String::from("abc")), Ok(String::from("f")));
    let mut account = Account::new(1000);
    let read = decode::<Containers>(encode(&value)?.bytes(), &mut account)?;
    assert_eq!(read.value, value);
    assert_eq!(account.remaining(), 1000 - "abc".len() - "f".len());
    Ok(())
}

#[test]
fn a_string_read_into_a_shared_string_is_charged_for_its_copy_too() -> TestResult {
    let mut account = Account::new(1000);
    let value = FieldText::BeforeCursor("Lis".into());
    let read = decode::<FieldText>(encode(&value)?.bytes(), &mut account)?;
    assert_eq!(read.value, value);
    let counts = 2 * size_of::<usize>();
    let shared = (counts + "Lis".len()).next_multiple_of(align_of::<usize>());
    assert_eq!(read.charge.bytes(), "Lis".len() + shared);
    Ok(())
}

#[test]
fn a_map_fails_to_decode() -> TestResult {
    let bytes = encode(&BTreeMap::from([(1_u32, 2_u32)]))?;
    assert!(matches!(
        decode::<BTreeMap<u32, u32>>(bytes.bytes(), &mut Account::new(1000)),
        Err(ProtocolError::Uncharged(Uncharged::Map))
    ));
    Ok(())
}

#[test]
fn a_decode_past_the_account_fails_and_charges_nothing() -> TestResult {
    let bytes = encode(&"x".repeat(64))?;
    let mut account = Account::new(63);
    assert!(matches!(
        decode::<String>(bytes.bytes(), &mut account),
        Err(ProtocolError::Charge {
            requested: 64,
            remaining: 63
        })
    ));
    assert_eq!(account.remaining(), 63);
    Ok(())
}

#[test]
fn a_released_charge_gives_its_memory_back_to_the_account() -> TestResult {
    let mut account = Account::new(10);
    let read = decode::<String>(encode(&"abc")?.bytes(), &mut account)?;
    assert_eq!(account.remaining(), 7);
    account.release(read.charge);
    assert_eq!(account.remaining(), 10);
    Ok(())
}

#[test]
fn a_sequence_is_charged_for_its_whole_buffer_before_its_first_element() -> TestResult {
    let elements: &[u64] = &[1, 2, 3];
    let bytes = encode(&elements)?;
    let mut account = Account::new(2 * size_of::<u64>());
    assert!(matches!(
        decode::<Vec<u64>>(bytes.bytes(), &mut account),
        Err(ProtocolError::Charge {
            requested: 24,
            remaining: 16
        })
    ));
    Ok(())
}

#[test]
fn a_sequence_longer_than_its_input_fails_before_it_is_charged() -> TestResult {
    let mut bytes = encode(&1_000_000_usize)?.bytes().to_vec();
    bytes.extend(encode(&[1_u64, 2])?.bytes());
    let mut account = Account::new(usize::MAX);
    assert!(matches!(
        decode::<Vec<u64>>(&bytes, &mut account),
        Err(ProtocolError::Uncharged(Uncharged::Length))
    ));
    assert_eq!(account.remaining(), usize::MAX);
    Ok(())
}

#[test]
fn bytes_after_the_end_of_a_message_fail_the_decode() -> TestResult {
    let mut bytes = encode(&1_u32)?.bytes().to_vec();
    bytes.push(0);
    assert!(matches!(
        decode::<u32>(&bytes, &mut Account::new(1000)),
        Err(ProtocolError::Finish { trailing: 1 })
    ));
    Ok(())
}

#[test]
fn malformed_bytes_fail_to_decode() {
    assert!(matches!(
        decode::<bool>(&[2], &mut Account::new(1000)),
        Err(ProtocolError::Decode(_))
    ));
}
