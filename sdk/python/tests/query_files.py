"""Cached SQL fixtures owned by the Python tests."""

from functools import cache
from pathlib import Path


@cache
def test_query(name: str) -> str:
    return Path(__file__).with_name("queries").joinpath(name).read_text(encoding="utf-8")


test_query.__test__ = False
