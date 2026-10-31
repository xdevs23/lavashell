// SPDX-License-Identifier: GPL-3.0-only
//! The rules that every platform shares: the record of each surface with its view, size, density and lifecycle, and
//! when a run ends.
#![forbid(unsafe_code)]

mod error;
mod lifecycle;
mod run_end;
mod surface_records;

pub use error::Error;
pub use run_end::RunEnd;
pub use surface_records::{SurfaceRecords, initial_size, measure_content};
