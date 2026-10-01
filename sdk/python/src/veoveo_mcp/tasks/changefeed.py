"""Native SurrealDB commit pages and typed Task identities.

The 3.3 single-node oracle and whole-transaction page rule match Rust Store.
Public readers select current admitted rows using these identities as hints.
"""
from __future__ import annotations

import asyncio
import uuid
from dataclasses import dataclass
from datetime import datetime
from typing import TYPE_CHECKING, Any

from surrealdb import RecordID

from .types import InvalidRecord, TaskUpdateCursor, parse_task_id

if TYPE_CHECKING:
    from .store import SurrealStore

PAGE_LIMIT = 1_000
_RETENTION_SAFETY_MS = 6 * 24 * 60 * 60 * 1_000


@dataclass(frozen=True)
class NativeChange:
    record: RecordID
    current: dict[str, Any] | None = None
    original: dict[str, Any] | None = None

    @classmethod
    def decode(cls, value: Any) -> NativeChange | None:
        if not isinstance(value, dict):
            raise InvalidRecord("invalid native changefeed entry")
        if "define_table" in value:
            return None
        if "delete" in value:
            deletion = value["delete"]
            record = deletion.get("id") if isinstance(deletion, dict) else deletion
            original = deletion.get("original") if isinstance(deletion, dict) else None
            if not isinstance(record, RecordID):
                raise InvalidRecord("native delete requires a record identity")
            return cls(record, original=original)
        for key in ("current", "create", "update"):
            row = value.get(key)
            if isinstance(row, dict) and isinstance(row.get("id"), RecordID):
                return cls(row["id"], current=row)
        raise InvalidRecord("unrecognized native changefeed entry")

    def task_id(self) -> uuid.UUID | None:
        if self.record.table_name != "task":
            return None
        if not isinstance(self.record.id, uuid.UUID):
            raise InvalidRecord("native Task change requires a UUID key")
        return parse_task_id(self.record.id)


@dataclass(frozen=True)
class ChangefeedBatch:
    cursor: TaskUpdateCursor
    changes: tuple[NativeChange, ...]

    @property
    def next_cursor(self) -> TaskUpdateCursor:
        return TaskUpdateCursor(self.cursor.versionstamp + 1)

    @classmethod
    def decode(cls, value: Any) -> ChangefeedBatch:
        if not isinstance(value, dict) or not isinstance(value.get("changes"), list):
            raise InvalidRecord("invalid native changefeed batch")
        return cls(TaskUpdateCursor(value.get("versionstamp")), tuple(
            change for item in value["changes"]
            if (change := NativeChange.decode(item)) is not None
        ))


async def _page(store: SurrealStore, cursor: TaskUpdateCursor, limit: int) -> list[ChangefeedBatch]:
    if not isinstance(cursor, TaskUpdateCursor):
        raise InvalidRecord("native changefeed requires a typed cursor")
    if type(limit) is not int or not 1 <= limit <= PAGE_LIMIT:
        raise InvalidRecord("native changefeed limit must be 1..1000")
    # SHOW does not accept bound cursor/limit values. Both constructors admit
    # integers only; no caller text or table identifier enters this statement.
    rows = await store.query(
        f"SHOW CHANGES FOR DATABASE SINCE {cursor.versionstamp} LIMIT {limit};"
    )
    return [ChangefeedBatch.decode(row) for row in rows[0] or []]


async def replay_changes(store: SurrealStore, cursor: TaskUpdateCursor, limit: int = PAGE_LIMIT) -> list[ChangefeedBatch]:
    batches = await _page(store, cursor, limit)
    if batches:
        tail = await _page(store, batches[-1].cursor, PAGE_LIMIT)
        if not tail or tail[0].cursor != batches[-1].cursor:
            raise InvalidRecord("native changefeed transaction tail disappeared")
        if len({change.record.table_name for change in tail[0].changes}) >= PAGE_LIMIT:
            raise InvalidRecord("native changefeed transaction exceeds table bound")
        batches[-1] = tail[0]
    return batches


async def cursor_now(store: SurrealStore) -> TaskUpdateCursor:
    result = (await store.query("RETURN time::now();"))[0]
    if not isinstance(result, datetime):
        raise InvalidRecord("native cursor requires database time")
    return TaskUpdateCursor(max(0, int(result.timestamp() * 1000) - 2_000) << 16)


async def changefeed_head(store: SurrealStore) -> TaskUpdateCursor:
    async with asyncio.timeout(15):
        cursor = await cursor_now(store)
        while batches := await replay_changes(store, cursor):
            next_cursor = batches[-1].next_cursor
            if next_cursor.versionstamp <= cursor.versionstamp:
                raise InvalidRecord("native changefeed cursor did not advance")
            cursor = next_cursor
        return cursor


def expired(cursor: TaskUpdateCursor, now: TaskUpdateCursor) -> bool:
    return cursor.versionstamp == 0 or (
        (now.versionstamp >> 16) - (cursor.versionstamp >> 16) >= _RETENTION_SAFETY_MS
    )
