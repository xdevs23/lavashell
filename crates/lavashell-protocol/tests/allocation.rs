// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]
//! Decoding a sample allocates no more than its account is charged, measured by a heap profiler that counts every
//! allocation of the process. The profiler is the global allocator, so this test has a binary of its own.

mod samples;

use lavashell_protocol::{Account, ProtocolError, decode};
use samples::{Check, TestResult, check_every_sample};
use serde::{Serialize, de::DeserializeOwned};
use std::path::Path;

#[global_allocator]
static ALLOCATOR: dhat::Alloc = dhat::Alloc;

/// The bytes that `run` allocates, with what it returns.
fn allocated<R>(run: impl FnOnce() -> R) -> (u64, R) {
    let before = dhat::HeapStats::get().total_bytes;
    let result = run();
    (dhat::HeapStats::get().total_bytes - before, result)
}

struct WithinAccount;

impl Check for WithinAccount {
    fn samples<T: Serialize + DeserializeOwned>(&mut self, samples: &[(T, &[u8])]) -> TestResult {
        for (index, (_, stored)) in samples.iter().enumerate() {
            let (bytes, decoded) = allocated(|| decode::<T>(stored, &mut Account::new(usize::MAX)));
            let charged = decoded?.charge.bytes();
            let limit = u64::try_from(charged)?;
            dhat::assert!(
                bytes <= limit,
                "sample {index} allocated {bytes} bytes and was charged {charged}"
            );
            if let Some(short) = charged.checked_sub(1) {
                let (bytes, failed) = allocated(|| decode::<T>(stored, &mut Account::new(short)));
                dhat::assert!(matches!(failed, Err(ProtocolError::Charge { .. })));
                let limit = u64::try_from(short)?;
                dhat::assert!(
                    bytes <= limit,
                    "sample {index} allocated {bytes} bytes past an account of {short}"
                );
            }
        }
        Ok(())
    }
}

#[test]
fn decoding_every_sample_allocates_no_more_than_its_account_is_charged() -> TestResult {
    let _profiler = dhat::Profiler::builder()
        .testing()
        .file_name(Path::new(env!("CARGO_TARGET_TMPDIR")).join("dhat-heap.json"))
        .build();
    check_every_sample(&mut WithinAccount)
}
