"""Offset cursor pagination compatible with the Rust `mcp-contract` scheme."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Sequence, TypeVar

CURSOR_PREFIX = "v1:"

T = TypeVar("T")


class PaginationError(ValueError):
    pass


@dataclass(frozen=True)
class Page[T]:
    items: list[T]
    next_cursor: str | None


def paginate(items: Sequence[T], cursor: str | None, page_size: int) -> Page[T]:
    if page_size <= 0:
        raise PaginationError("page size must be greater than zero")
    start = _decode_cursor(cursor) if cursor is not None else 0
    total = len(items)
    next_offset = start + page_size
    next_cursor = f"{CURSOR_PREFIX}{next_offset}" if next_offset < total else None
    return Page(items=list(items[start : start + page_size]), next_cursor=next_cursor)


def _decode_cursor(cursor: str) -> int:
    if not cursor.startswith(CURSOR_PREFIX):
        raise PaginationError(f"invalid pagination cursor {cursor!r}")
    offset = cursor[len(CURSOR_PREFIX) :]
    if not offset.isdigit():
        raise PaginationError(f"invalid pagination cursor {cursor!r}")
    return int(offset)
