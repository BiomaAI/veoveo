"""Owned Task notification readers with SQL admission on every current-state read."""

from __future__ import annotations

import uuid
from collections import deque
from dataclasses import dataclass
from datetime import datetime, timezone
from time import monotonic
from typing import Any, Sequence

from .owner_query import OwnerTaskQuery, native_task_id
from .runtime import _record_to_snapshot
from .store import OutboxWake
from .types import InvalidRecord, TaskError, TaskSnapshot, TaskUpdate, TaskUpdateCursor, task_record

_PAGE_SIZE = 256
_RECONCILE_SECONDS = 15.0
_WAKE_SECONDS = 2.0


@dataclass(frozen=True)
class OwnerTaskSubscription:
    accepted_task_ids: tuple[uuid.UUID, ...]
    updates: OwnerTaskUpdates


@dataclass(frozen=True)
class _UpdatePage:
    cursor: TaskUpdateCursor
    tasks: tuple[TaskSnapshot, ...]
    full: bool

    @classmethod
    def from_store(cls, value: dict[str, Any]) -> _UpdatePage:
        sequence = value["cursor"] if value["cursor"] is not None else 0
        if type(sequence) is not int or type(value.get("full", False)) is not bool:
            raise InvalidRecord("invalid owner Task update position")
        return cls(
            TaskUpdateCursor(sequence),
            tuple(_record_to_snapshot(record) for record in value["tasks"]),
            value.get("full", False),
        )


async def _baseline(query: OwnerTaskQuery, ids: Sequence[uuid.UUID]) -> _UpdatePage:
    rows = await query.runtime.store.query(
        "RETURN { cursor: array::first((SELECT VALUE sequence FROM outbox_event "
        "WHERE available_at <= $now ORDER BY sequence DESC LIMIT 1)), "
        f"tasks: (SELECT * FROM $records WHERE {query.predicate()}) }};",
        {**query.bindings(), "records": [task_record(task_id) for task_id in ids],
         "now": datetime.now(timezone.utc)},
    )
    return _UpdatePage.from_store(rows[0])


async def subscribe(query: OwnerTaskQuery, ids: Sequence[uuid.UUID]) -> OwnerTaskSubscription:
    if len(ids) > _PAGE_SIZE:
        raise TaskError("Task subscription accepts at most 256 identities")
    ids = tuple(sorted({native_task_id(task_id) for task_id in ids}))
    if not ids:
        return OwnerTaskSubscription((), OwnerTaskUpdates(query, (), None, 0, []))
    wake = await query.runtime.store.outbox_wake()
    try:
        baseline = await _baseline(query, ids)
        initial = baseline.tasks
        accepted = tuple(snapshot.task_id for snapshot in initial)
        if not accepted:
            await wake.close()
            wake = None
        updates = OwnerTaskUpdates(query, accepted, wake, baseline.cursor.sequence, initial)
        return OwnerTaskSubscription(accepted, updates)
    except BaseException:
        if wake is not None:
            await wake.close()
        raise


class OwnerTaskUpdates:
    """Closeable even when the consumer never begins iteration."""

    def __init__(
        self, query: OwnerTaskQuery, ids: tuple[uuid.UUID, ...],
        wake: OutboxWake | None, sequence: int, initial: Sequence[TaskSnapshot],
    ) -> None:
        self._query = query
        self._ids = ids
        self._wake = wake
        self._cursor = TaskUpdateCursor(sequence)
        self._pending = deque(TaskUpdate(self._cursor, snapshot) for snapshot in initial)
        self._reconcile_at = monotonic() + _RECONCILE_SECONDS
        self._closed = False
        self._replay_pending = False

    def __aiter__(self) -> OwnerTaskUpdates:
        return self

    async def __anext__(self) -> TaskUpdate:
        try:
            while not self._closed:
                if self._pending:
                    return self._pending.popleft()
                if self._wake is None:
                    raise StopAsyncIteration
                if not self._replay_pending:
                    await self._wake.wait(_WAKE_SECONDS)
                if monotonic() >= self._reconcile_at:
                    baseline = await _baseline(self._query, self._ids)
                    self._append(baseline)
                    self._replay_pending = False
                    self._reconcile_at = monotonic() + _RECONCILE_SECONDS
                else:
                    await self._replay()
            raise StopAsyncIteration
        except BaseException:
            await self.aclose()
            raise

    def _append(self, page: _UpdatePage) -> None:
        self._cursor = TaskUpdateCursor(max(self._cursor.sequence, page.cursor.sequence))
        self._pending.extend(
            TaskUpdate(self._cursor, snapshot) for snapshot in page.tasks
        )

    async def _replay(self) -> None:
        query = self._query
        rows = await query.runtime.store.query(
            "LET $changes = SELECT sequence, aggregate_id FROM outbox_event "
            "WHERE sequence > $cursor AND available_at <= $now "
            "AND aggregate_type = 'task' AND aggregate_id IN $ids "
            "ORDER BY sequence ASC LIMIT $limit; "
            "RETURN { cursor: array::last($changes.sequence), "
            "full: array::len($changes) = $limit, "
            f"tasks: (SELECT * FROM $records WHERE {query.predicate()} "
            "AND <string> record::id(id) IN $changes.aggregate_id) };",
            {**query.bindings(), "cursor": self._cursor.sequence,
             "now": datetime.now(timezone.utc), "limit": _PAGE_SIZE,
             "ids": [str(task_id) for task_id in self._ids],
             "records": [task_record(task_id) for task_id in self._ids]},
        )
        page = _UpdatePage.from_store(rows[1])
        self._append(page)
        self._replay_pending = page.full

    async def aclose(self) -> None:
        if self._closed:
            return
        self._closed = True
        self._pending.clear()
        if self._wake is not None:
            await self._wake.close()
