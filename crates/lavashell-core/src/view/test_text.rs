// SPDX-License-Identifier: GPL-3.0-only

use crate::units::Dp;

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
