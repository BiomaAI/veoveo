from pathlib import Path
import tempfile
import unittest

from map_data.subprocesses import ToolFailure, run_tool


class SubprocessTests(unittest.TestCase):
    def test_enforces_elapsed_time_and_terminates_process_group(self):
        with tempfile.TemporaryDirectory() as root:
            with self.assertRaises(ToolFailure) as raised:
                run_tool(
                    "/bin/sh",
                    ["-c", "sleep 5"],
                    timeout_seconds=1,
                    cwd=Path(root),
                )
            self.assertIn("time limit", str(raised.exception))

    def test_bounds_diagnostic_output(self):
        with tempfile.TemporaryDirectory() as root:
            with self.assertRaises(ToolFailure) as raised:
                run_tool(
                    "/bin/sh",
                    ["-c", "printf 123456789"],
                    timeout_seconds=5,
                    cwd=Path(root),
                    maximum_log_bytes=4,
                )
            self.assertIn("diagnostic-output limit", str(raised.exception))


if __name__ == "__main__":
    unittest.main()
