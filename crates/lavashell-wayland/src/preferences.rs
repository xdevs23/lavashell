// SPDX-License-Identifier: GPL-3.0-only

mod portal;
#[cfg(test)]
pub(crate) mod test_error;
#[cfg(test)]
mod tests;

pub(crate) use portal::watch;
