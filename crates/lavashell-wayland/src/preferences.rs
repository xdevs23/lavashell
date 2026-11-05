// SPDX-License-Identifier: GPL-3.0-only
//! Reads the appearance preferences of the Linux desktop from the Settings portal on D-Bus, without blocking the UI
//! thread.

mod portal;
#[cfg(test)]
mod tests;

pub(crate) use portal::watch;
