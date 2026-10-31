// SPDX-License-Identifier: GPL-3.0-only
//! The rules that every platform shares: the record of each surface with its view, size, density and lifecycle, the
//! description of each window and its shape, the record of each output with the surfaces shown on it, the routing
//! of pointer and keyboard input to the views, and when a run ends.
#![forbid(unsafe_code)]

mod error;
mod input;
mod lifecycle;
mod output_records;
mod run_end;
mod shape;
mod surface_records;
#[cfg(test)]
mod test_text;
mod window;

pub use error::Error;
pub use input::{InputFilter, InputRouter, RoutedInput};
pub use output_records::OutputRecords;
pub use run_end::RunEnd;
pub use shape::WindowShape;
pub use surface_records::{SurfaceRecords, measure_content};
pub use window::{DragRegion, Window};
