// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]

use std::error::Error;
use std::fmt;

pub mod next_step;

pub fn message_chain(error: impl Error) -> String {
    std::iter::successors(error.source(), |&cause| cause.source())
        .fold(error.to_string(), |text, cause| format!("{text}: {cause}"))
}

pub struct InOrder<'a, E>(pub &'a [E]);

impl<E: Error> fmt::Display for InOrder<'_, E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, failure) in self.0.iter().enumerate() {
            if index > 0 {
                formatter.write_str("; after that, ")?;
            }
            formatter.write_str(&message_chain(failure))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{InOrder, message_chain};
    use std::{error::Error, fmt};

    #[derive(Debug)]
    struct Failure(&'static str, Option<Box<Failure>>);
    impl fmt::Display for Failure {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str(self.0)
        }
    }
    impl Error for Failure {
        fn source(&self) -> Option<&(dyn Error + 'static)> {
            self.1.as_deref().map(|cause| cause as &dyn Error)
        }
    }

    #[test]
    fn follows_every_cause_in_order() {
        let root = Failure("root", None);
        let middle = Failure("middle", Some(Box::new(root)));
        let error = Failure("outer", Some(Box::new(middle)));
        assert_eq!(message_chain(&error), "outer: middle: root");
        assert_eq!(message_chain(Failure("alone", None)), "alone");
    }

    #[test]
    fn joins_the_cause_chains_of_several_failures_in_order() {
        let first = Failure("first", Some(Box::new(Failure("cause", None))));
        let failures = [first, Failure("second", None)];
        assert_eq!(
            InOrder(&failures).to_string(),
            "first: cause; after that, second"
        );
    }
}
