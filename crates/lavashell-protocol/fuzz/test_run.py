# SPDX-License-Identifier: GPL-3.0-only
"""Checks of how the fuzz run shares the machine between its fuzzers."""

import unittest

from run import assignments, largest_input


class ShareTests(unittest.TestCase):
    def test_each_cpu_fuzzes_the_targets_in_turn_and_every_target_gets_a_fuzzer(self):
        self.assertEqual(assignments(["decode", "apply"], 3), ["decode", "apply", "decode"])
        self.assertEqual(assignments(["decode", "apply"], 1), ["decode", "apply"])

    def test_the_longest_input_takes_half_of_the_memory_share_of_a_fuzzer(self):
        self.assertEqual(largest_input(4000, 4), 500)
        self.assertEqual(largest_input(3, 4), 1)


if __name__ == "__main__":
    unittest.main()
