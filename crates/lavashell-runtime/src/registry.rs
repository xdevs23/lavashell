// SPDX-License-Identifier: GPL-3.0-only
//! The optional subsystems of the runtime. Each one registers the platform capability it needs and a check whether a
//! setup uses that capability.

use crate::run::RuntimeSetup;
use lavashell_platform::Capability;
use std::collections::BTreeSet;
use std::fmt;

pub(crate) struct Subsystem {
    pub capability: Capability,
    pub used: fn(&RuntimeSetup) -> bool,
}

inventory::collect!(Subsystem);

/// Every capability used by `setup` and missing from `offered`.
pub(crate) fn missing(
    setup: &RuntimeSetup,
    offered: &BTreeSet<Capability>,
) -> BTreeSet<Capability> {
    (inventory::iter::<Subsystem>.into_iter())
        .filter(|subsystem| !offered.contains(&subsystem.capability) && (subsystem.used)(setup))
        .map(|subsystem| subsystem.capability)
        .collect()
}

/// Shows capabilities separated by commas, in `Capability` order.
pub(crate) struct Listed<'a>(pub &'a BTreeSet<Capability>);

impl fmt::Display for Listed<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, capability) in self.0.iter().enumerate() {
            if index > 0 {
                formatter.write_str(", ")?;
            }
            write!(formatter, "{capability}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
