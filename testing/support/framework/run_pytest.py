"""Record outcomes from maintained pytest hooks for one exact owned node ID."""
import json
import os
from pathlib import Path
import sys

import pytest


class Outcome:
    def __init__(self, expected):
        self.expected = expected
        self.matched = 0
        self.passed = 0
        self.skipped = 0
        self.failed = 0
        self.observed = None

    def pytest_collection_finish(self, session):
        self.matched = len(session.items)
        if self.matched != 1:
            raise pytest.UsageError("framework selection must collect exactly one case")
        observed = session.items[0].nodeid
        expected_path, expected_separator, expected_case = self.expected.partition("::")
        observed_path, observed_separator, observed_case = observed.partition("::")
        if (not expected_separator or not observed_separator
                or Path(expected_path).resolve() != (session.config.rootpath / observed_path).resolve()
                or expected_case != observed_case):
            raise pytest.UsageError("collected node identity differs from owned selection")
        self.observed = observed

    def pytest_runtest_logreport(self, report):
        if report.nodeid != self.observed:
            self.failed += 1
        elif report.failed:
            self.failed += 1
        elif report.skipped:
            self.skipped += 1
        elif report.when == "call" and report.passed:
            self.passed += 1


def main():
    if len(sys.argv) != 3:
        raise SystemExit("pytest delivery requires exact node ID and private report")
    expected, destination = sys.argv[1:]
    outcome = Outcome(expected)
    status = pytest.main(["--strict-markers", "--tb=short", "-q", expected], plugins=[outcome])
    report = dict(format="veoveo.ai/framework-outcome/v1", framework="pytest", case=expected,
                  matched=outcome.matched, passed=outcome.passed, skipped=outcome.skipped,
                  failed=outcome.failed)
    fd = os.open(destination, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(fd, "w", encoding="utf-8", newline="\n") as stream:
        json.dump(report, stream)
        stream.write("\n")
    if status != pytest.ExitCode.OK or (outcome.matched, outcome.passed, outcome.skipped, outcome.failed) != (1, 1, 0, 0):
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
