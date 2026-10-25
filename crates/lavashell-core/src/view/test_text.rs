// SPDX-License-Identifier: GPL-3.0-only
//! Arithmetic for the text measurers in the view tests, which compute the size of a text from its
//! number of characters or lines.

use crate::units::Dp;

/// The sum of `count` copies of `each`, such as the width of `count` characters that are each
/// `each` wide.
pub(crate) fn repeated(each: Dp, count: usize) -> Dp {
    (0..count).map(|_| each).sum()
}

mod tests {
    use super::repeated;
    use crate::units::DpExt;

    #[test]
    fn repeating_a_length_adds_it_that_many_times() {
        assert_eq!(repeated(10.dp(), 3), 30.dp());
        assert_eq!(repeated(10.dp(), 0), 0.dp());
    }
}
