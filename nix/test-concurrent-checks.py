#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only

import unittest
from threading import Event
from unittest.mock import patch

import concurrent_checks


def fail(error):
    raise error


class RunConcurrently(unittest.TestCase):
    def test_results_come_back_in_the_order_of_the_checks(self):
        with patch.object(concurrent_checks, "concurrent_count", return_value=3):
            self.assertEqual(concurrent_checks.run_concurrently([lambda value=value: value for value in range(5)]),
                             list(range(5)))

    def test_a_failure_cancels_the_checks_that_have_not_started(self):
        started = []
        checks = [lambda: fail(ValueError("first")), lambda: started.append("second")]
        with patch.object(concurrent_checks, "concurrent_count", return_value=1), \
                self.assertRaisesRegex(ValueError, "first"):
            concurrent_checks.run_concurrently(checks)
        self.assertEqual(started, [])

    def test_the_failure_of_the_first_failed_check_in_order_is_raised(self):
        second_started, third_failed = Event(), Event()

        def second():
            second_started.set()
            third_failed.wait()
            raise ValueError("second")

        def third():
            second_started.wait()
            third_failed.set()
            raise KeyError("third")

        checks = [lambda: 0, second, third]
        with patch.object(concurrent_checks, "concurrent_count", return_value=3), \
                self.assertRaisesRegex(ValueError, "second"):
            concurrent_checks.run_concurrently(checks)

    def test_no_checks_give_no_results(self):
        self.assertEqual(concurrent_checks.run_concurrently([]), [])


if __name__ == "__main__":
    unittest.main()
