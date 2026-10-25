// SPDX-License-Identifier: GPL-3.0-only
use std::error::Error;

/// The message of `error` followed by the message of each of its causes, joined by colons. Code that passes an
/// error out of Rust, such as into a log or a Java exception, uses this text because the receiver gets only one
/// message.
pub fn message_chain(error: impl Error) -> String {
    std::iter::successors(error.source(), |&cause| cause.source())
        .fold(error.to_string(), |text, cause| format!("{text}: {cause}"))
}

#[cfg(test)]
mod tests {
    use super::message_chain;
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
}
