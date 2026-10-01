"""Owned Task notifications with SQL admission on every current-state read."""

from __future__ import annotations

import uuid
from collections import deque
from dataclasses import dataclass
from typing import Sequence

from .changefeed import changefeed_head, cursor_now, expired, replay_changes
from .owner_query import OwnerTaskQuery, native_task_id
from .runtime import _record_to_snapshot
from .store import NativeWake
from .types import TaskError, TaskSnapshot, TaskUpdate, TaskUpdateCursor, task_record

_MAX_IDENTITIES = 256


@dataclass(frozen=True)
class OwnerTaskSubscription:
    accepted_task_ids: tuple[uuid.UUID, ...]
    updates: OwnerTaskUpdates


async def _current(query: OwnerTaskQuery, ids: Sequence[uuid.UUID]) -> tuple[TaskSnapshot, ...]:
    rows = await query.runtime.store.query(
        f"SELECT * FROM $records WHERE {query.predicate()};",
        {**query.bindings(), "records": [task_record(task_id) for task_id in ids]},
    )
    return tuple(_record_to_snapshot(record) for record in rows[0] or [])


async def subscribe(query: OwnerTaskQuery, ids: Sequence[uuid.UUID]) -> OwnerTaskSubscription:
    if len(ids) > _MAX_IDENTITIES:
        raise TaskError("Task subscription accepts at most 256 identities")
    ids = tuple(sorted({native_task_id(task_id) for task_id in ids}))
    if not ids:
        return OwnerTaskSubscription((), OwnerTaskUpdates(query, (), None, TaskUpdateCursor(0), ()))
    wake = await query.runtime.store.task_wake()
    try:
        cursor = await changefeed_head(query.runtime.store)
        initial = await _current(query, ids)
        accepted = tuple(snapshot.task_id for snapshot in initial)
        if not accepted:
            await wake.close()
            wake = None
        return OwnerTaskSubscription(
            accepted, OwnerTaskUpdates(query, accepted, wake, cursor, initial)
        )
    except BaseException:
        if wake is not None:
            await wake.close()
        raise


class OwnerTaskUpdates:
    """Closeable before iteration. A lost connection requires fresh admission."""

    def __init__(
        self, query: OwnerTaskQuery, ids: tuple[uuid.UUID, ...],
        wake: NativeWake | None, cursor: TaskUpdateCursor, initial: Sequence[TaskSnapshot],
    ) -> None:
        self._query = query
        self._ids = ids
        self._wake = wake
        self._cursor = cursor
        self._pending = deque(TaskUpdate(cursor, snapshot) for snapshot in initial)
        self._closed = False
        # Drain after registration/baseline to cover writes before subscribe_live
        # attached its SDK queue, even when no later LIVE hint arrives.
        self._replay_pending = True

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
                    await self._wake.wait()
                await self._replay()
            raise StopAsyncIteration
        except BaseException:
            await self.aclose()
            raise

    async def _replay(self) -> None:
        store = self._query.runtime.store
        now = await cursor_now(store)
        if expired(self._cursor, now):
            self._cursor = await changefeed_head(store)
            snapshots = await _current(self._query, self._ids)
            self._replay_pending = True
        else:
            batches = await replay_changes(store, self._cursor)
            self._replay_pending = bool(batches)
            if not batches:
                return
            self._cursor = batches[-1].next_cursor
            changed = {
                task_id for batch in batches for change in batch.changes
                if (task_id := change.task_id()) in self._ids
            }
            snapshots = await _current(self._query, sorted(changed)) if changed else ()
        self._pending.extend(TaskUpdate(self._cursor, snapshot) for snapshot in snapshots)

    async def aclose(self) -> None:
        if self._closed:
            return
        self._closed = True
        self._pending.clear()
        if self._wake is not None:
            await self._wake.close()
