// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]

mod checked_command;
mod error;
mod process_exit;
mod process_group;
mod readiness;
mod scoped_child;
mod termination;

pub use checked_command::{CommandLine, run_checked, run_checked_for_stdout};
pub use error::ProcessError;
pub use process_exit::ProcessExit;
pub use readiness::first_ready;
pub use scoped_child::ScopedChild;
pub use termination::{RunOutcome, Termination, run_ending_groups_on_termination};
