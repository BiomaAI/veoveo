"""Trusted worker replay. Public readers use current owner SQL admission."""

from __future__ import annotations

from collections import deque
from typing import TYPE_CHECKING

from .changefeed import changefeed_head, cursor_now, expired, replay_changes
from .runtime import _record_to_snapshot
from .store import NativeWake
from .types import TaskSnapshot, TaskUpdate, TaskUpdateCursor, server_record

if TYPE_CHECKING:
    from .runtime import TaskRuntime


async def history_updates(
    runtime: TaskRuntime, after: TaskUpdateCursor | None = None,
) -> TaskHistory:
    wake = await runtime.store.task_wake()
    try:
        head = await changefeed_head(runtime.store)
        cursor = after if after is not None else head
        initial = []
        if after is None or expired(cursor, head):
            cursor = head
            initial = await runtime.list()
        return TaskHistory(runtime, wake, cursor, initial)
    except BaseException:
        await wake.close()
        raise


class TaskHistory:
    """Replay complete commits; returned cursors deliberately repeat their commit.

    The worker owns persistence of its delivered cursor. Connection loss closes
    this reader; live_updates_after resumes from the worker's last acknowledgement.
    A cursor outside retention starts with the current server Task baseline.
    """

    def __init__(
        self, runtime: TaskRuntime, wake: NativeWake, cursor: TaskUpdateCursor,
        initial: list[TaskSnapshot],
    ) -> None:
        self._runtime = runtime
        self._wake = wake
        self._cursor = cursor
        self._pending = deque(TaskUpdate(cursor, snapshot) for snapshot in initial)
        self._closed = False
        self._replay_pending = True

    def __aiter__(self) -> TaskHistory:
        return self

    async def __anext__(self) -> TaskUpdate:
        try:
            while not self._closed:
                if self._pending:
                    return self._pending.popleft()
                if not self._replay_pending:
                    await self._wake.wait()
                store = self._runtime.store
                if expired(self._cursor, await cursor_now(store)):
                    self._cursor = await changefeed_head(store)
                    self._pending.extend(
                        TaskUpdate(self._cursor, snapshot)
                        for snapshot in await self._runtime.list()
                    )
                    self._replay_pending = True
                    continue
                batches = await replay_changes(store, self._cursor)
                self._replay_pending = bool(batches)
                for batch in batches:
                    for change in batch.changes:
                        row = change.current
                        if change.record.table_name != "task" or row is None:
                            continue
                        if row.get("server") != server_record(self._runtime.server):
                            continue
                        self._pending.append(TaskUpdate(batch.cursor, _record_to_snapshot(row)))
                    self._cursor = batch.next_cursor
            raise StopAsyncIteration
        except BaseException:
            await self.aclose()
            raise

    async def aclose(self) -> None:
        if self._closed:
            return
        self._closed = True
        self._pending.clear()
        await self._wake.close()
