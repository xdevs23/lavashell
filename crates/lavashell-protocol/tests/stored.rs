// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]
//! Every sample encodes to its stored bytes, and the stored bytes decode to a value with those bytes.

mod samples;

use lavashell_protocol::{Hello, encode};
use samples::{Check, TestResult, check_every_sample, hello, read};
use serde::{Serialize, de::DeserializeOwned};

struct StoredBytes;

impl Check for StoredBytes {
    fn samples<T: Serialize + DeserializeOwned>(&mut self, samples: &[(T, &[u8])]) -> TestResult {
        for (index, (sample, stored)) in samples.iter().enumerate() {
            assert_eq!(encode(sample)?.bytes(), *stored, "sample {index}");
            assert_eq!(
                encode(&read::<T>(stored)?)?.bytes(),
                *stored,
                "sample {index}"
            );
        }
        Ok(())
    }
}

#[test]
fn every_sample_encodes_to_its_stored_bytes_and_decodes_to_a_value_with_those_bytes() -> TestResult
{
    check_every_sample(&mut StoredBytes)
}

#[test]
fn the_stored_hello_decodes_to_an_equal_hello() -> TestResult {
    let (hello, stored) = hello()?;
    assert_eq!(read::<Hello>(stored)?, hello);
    Ok(())
}
