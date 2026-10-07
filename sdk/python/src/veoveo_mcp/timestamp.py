"""Immutable Chrono JSON timestamps without datetime precision or year limits."""
from __future__ import annotations

import calendar
import re
from dataclasses import dataclass
from datetime import datetime, timedelta, timezone

from pydantic_core import core_schema

# Qualified Chrono JSON profile: emitted padded date/time and standard offsets.
_PATTERN = (
    r"(?P<year>[+-][0-9]{4,6}|[0-9]{4})-(?P<month>0[1-9]|1[0-2])-(?P<day>0[1-9]|[12][0-9]|3[01])"
    r"T(?P<hour>[01][0-9]|2[0-3]):(?P<minute>[0-5][0-9]):(?P<second>[0-5][0-9]|60)"
    r"(?:\.(?P<fraction>[0-9]{1,9}))?"
    r"(?P<zone>Z|(?P<sign>[+-])(?P<offset_hour>[01][0-9]|2[0-3]):(?P<offset_minute>[0-5][0-9]))"
)


def _day(year: int, month: int, day: int) -> int:
    """Proleptic Gregorian day; calendar supports signed and extended years."""
    return (365 * (year - 1) + calendar.leapdays(1, year)
            + sum(calendar.monthrange(year, m)[1] for m in range(1, month)) + day - 1)


_EPOCH_DAY = _day(1970, 1, 1)
_MIN_SECONDS = (_day(-262143, 1, 1) - _EPOCH_DAY) * 86400
_MAX_SECONDS = (_day(262142, 12, 31) - _EPOCH_DAY) * 86400 + 86399


def _instant(wire: str) -> tuple[int, int]:
    if not isinstance(wire, str):
        raise ValueError("Chrono timestamp requires text; use from_datetime for a clock value")
    match = re.fullmatch(_PATTERN, wire)
    if match is None:
        raise ValueError("timestamp requires the qualified timezone-bearing Chrono Gregorian string profile")
    year, month, day, hour, minute, second = (
        int(match[name]) for name in ("year", "month", "day", "hour", "minute", "second")
    )
    if not (-262143 <= year <= 262142 and 1 <= month <= 12
            and 1 <= day <= calendar.monthrange(year, month)[1]
            and 0 <= hour <= 23 and 0 <= minute <= 59 and 0 <= second <= 60):
        raise ValueError("timestamp calendar component exceeds Chrono's range")
    offset = 0
    if match["sign"]:
        offset_hour, offset_minute = int(match["offset_hour"]), int(match["offset_minute"])
        if offset_hour > 23 or offset_minute > 59:
            raise ValueError("timestamp offset must be less than 24 hours")
        offset = (offset_hour * 60 + offset_minute) * 60
        if match["sign"] != "+":
            offset = -offset
    seconds = ((_day(year, month, day) - _EPOCH_DAY) * 86400
               + hour * 3600 + minute * 60 + min(second, 59) - offset)
    if not _MIN_SECONDS <= seconds <= _MAX_SECONDS:
        raise ValueError("timestamp UTC instant exceeds Chrono's range")
    nanos = int((match["fraction"] or "")[:9].ljust(9, "0"))
    if second == 60:
        nanos += 1_000_000_000
    return seconds, nanos


@dataclass(frozen=True, slots=True)
class ChronoTimestamp:
    """Checked original wire token; comparisons require explicit instant semantics.

    A leap second uses Chrono's preceding whole second plus >=1e9 nanoseconds.
    No default lexical ordering or datetime subclass is provided.
    """

    wire: str

    def __post_init__(self) -> None:
        _instant(self.wire)

    def __str__(self) -> str:
        return self.wire

    @classmethod
    def from_datetime(cls, value: datetime) -> ChronoTimestamp:
        if not isinstance(value, datetime) or value.tzinfo is None or value.utcoffset() is None:
            raise ValueError("timestamp construction requires an aware datetime")
        return cls(value.astimezone(timezone.utc).isoformat().replace("+00:00", "Z"))

    def instant_parts(self) -> tuple[int, int]:
        """UTC whole seconds and nanoseconds, with Chrono's explicit leap encoding."""
        return _instant(self.wire)

    def same_instant(self, other: ChronoTimestamp) -> bool:
        if not isinstance(other, ChronoTimestamp):
            raise TypeError("timestamp comparison requires another ChronoTimestamp")
        return self.instant_parts() == other.instant_parts()

    def as_datetime_exact(self) -> datetime:
        seconds, nanos = self.instant_parts()
        if nanos >= 1_000_000_000 or nanos % 1000:
            raise ValueError("datetime cannot exactly represent this leap second or nanosecond precision")
        return self._datetime(seconds, nanos)

    def as_datetime_lossy_microseconds(self) -> datetime:
        """Explicitly discard sub-microsecond digits; leap/extended years still refuse."""
        seconds, nanos = self.instant_parts()
        if nanos >= 1_000_000_000:
            raise ValueError("datetime cannot represent a leap second")
        return self._datetime(seconds, nanos)

    @staticmethod
    def _datetime(seconds: int, nanos: int) -> datetime:
        try:
            return datetime(1970, 1, 1, tzinfo=timezone.utc) + timedelta(
                seconds=seconds, microseconds=nanos // 1000
            )
        except OverflowError as error:
            raise ValueError("datetime cannot represent this timestamp year") from error

    @classmethod
    def __get_pydantic_core_schema__(cls, _source, _handler):
        def admit(value):
            # Reconstruct even nominal instances: forged objects cannot bypass checks.
            return cls(value.wire if isinstance(value, cls) else value)
        return core_schema.no_info_plain_validator_function(
            admit, json_schema_input_schema=core_schema.str_schema(strict=True),
            serialization=core_schema.plain_serializer_function_ser_schema(
                lambda value: admit(value).wire, return_schema=core_schema.str_schema(),
            ),
        )

    @classmethod
    def __get_pydantic_json_schema__(cls, schema, handler):
        result = handler(schema)
        result.update(
            type="string", pattern="^" + re.sub(r"\(\?P<[^>]+>", "(", _PATTERN) + r"$(?![\s\S])",
            description=("Chrono DateTime JSON string: proleptic Gregorian years -262143..262142, "
                         "timezone offset below 24 hours, nanoseconds and explicit leap seconds. "
                         "Calendar and UTC bounds are checked on admission. Original text is retained; "
                         "the qualified profile permits at most nine fractional digits."),
        )
        return result
