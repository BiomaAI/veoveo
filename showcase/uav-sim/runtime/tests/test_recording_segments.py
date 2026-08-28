from __future__ import annotations

import unittest

from veoveo_uav_sim.recording_segments import RecordingSegmentBudget


class RecordingSegmentBudgetTests(unittest.TestCase):
    def test_rotates_before_size_or_age_overflow(self) -> None:
        budget = RecordingSegmentBudget(
            maximum_bytes=80_000,
            maximum_seconds=60,
            opened_monotonic_s=100.0,
        )
        self.assertFalse(budget.should_rotate_before(4_000, 120.0))
        budget.account(4_000)
        self.assertTrue(budget.should_rotate_before(11_000, 120.0))
        self.assertTrue(budget.should_rotate_before(1, 160.0))

    def test_rejects_invalid_budget_inputs(self) -> None:
        with self.assertRaisesRegex(ValueError, "static context budget"):
            RecordingSegmentBudget(64 * 1024, 60, 100.0)
        budget = RecordingSegmentBudget(80_000, 60, 100.0)
        with self.assertRaisesRegex(ValueError, "clock moved backwards"):
            budget.should_rotate_before(1, 99.0)
        with self.assertRaisesRegex(ValueError, "must not be negative"):
            budget.account(-1)


if __name__ == "__main__":
    unittest.main()
