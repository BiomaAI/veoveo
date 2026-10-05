"""Lossless Task compare-and-set timestamps with a datetime presentation view."""

import calendar
import re
from datetime import datetime, timezone

from surrealdb import Datetime

from ..types import CheckedText


class TaskTimestamp(CheckedText):
    _pattern = r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{1,9})?(?:Z|[+-]\d{2}:\d{2})"

    @classmethod
    def _validate(cls, value: str) -> None:
        if not re.fullmatch(cls._pattern, value):
            raise ValueError("Task timestamp requires RFC3339 with at most nine fractional digits")
        datetime.fromisoformat(value.replace("Z", "+00:00"))

    @classmethod
    def __get_pydantic_json_schema__(cls, core_schema, handler):
        schema = handler(core_schema)
        schema.update(format="date-time", pattern=f"^{cls._pattern}$")
        return schema

    def as_datetime(self) -> datetime:
        return datetime.fromisoformat(self.replace("Z", "+00:00"))

    def driver_value(self) -> Datetime:
        return Datetime(str(self))

    def same_instant(self, other: "TaskTimestamp") -> bool:
        if not isinstance(other, TaskTimestamp):
            raise TypeError("Task timestamp comparison requires a TaskTimestamp")
        return self._instant() == other._instant()

    def _instant(self) -> tuple[int, int]:
        utc = self.as_datetime().astimezone(timezone.utc).replace(microsecond=0)
        fraction = re.search(r"\.(\d+)", self)
        nanoseconds = int(fraction.group(1).ljust(9, "0")) if fraction else 0
        return calendar.timegm(utc.timetuple()), nanoseconds
