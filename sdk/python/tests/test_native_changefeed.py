"""Pinned 3.3 commit paging, worker resume and notification liveness contracts."""

import asyncio
from datetime import timedelta

import pytest
from surrealdb import RecordID

from query_files import test_query
from veoveo_mcp.tasks import (
    InvalidRecord, StoreError, TaskRuntime, TaskStatus, TaskTransition, TaskUpdateCursor,
)
from veoveo_mcp.tasks.changefeed import (
    NativeChange, changefeed_head, replay_changes,
)
from veoveo_mcp.tasks.types import task_record
from test_owner_task_query import runtime  # noqa: F401 — isolated native fixture
from test_task_runtime_integration import draft, owner


@pytest.mark.parametrize("value", [-1, 2**63, True, "1", 1.5])
def test_native_cursor_rejects_unchecked_values(value):
    with pytest.raises(InvalidRecord):
        TaskUpdateCursor(value)


async def test_page_limit_preserves_every_table_in_the_final_transaction(runtime):
    async with asyncio.timeout(15):
        caller = owner()
        a, b = [(await runtime.create(draft(server=runtime.server, owner=caller))).snapshot for _ in range(2)]
        start = await changefeed_head(runtime.store)
        await runtime.store.query(
            test_query("test_native_changefeed/test_page_limit_preserves_every_table_in_the_final_transaction.surql"),
            {"a": task_record(a.task_id), "b": task_record(b.task_id),
             "principal": caller.principal_record()},
        )
        batches = await replay_changes(runtime.store, start, limit=1)
        assert len(batches) == 1
        assert {change.record.table_name for change in batches[0].changes} == {"task", "principal"}
        assert {change.task_id() for change in batches[0].changes if change.task_id()} == {a.task_id, b.task_id}
        assert await replay_changes(runtime.store, batches[-1].next_cursor) == []


async def test_worker_resume_repeats_whole_final_commit_and_stale_cursor_reconciles(runtime):
    async with asyncio.timeout(15):
        tasks = [(await runtime.create(draft(server=runtime.server))).snapshot for _ in range(2)]
        start = await changefeed_head(runtime.store)
        await runtime.store.query(
            test_query("test_native_changefeed/test_worker_resume_repeats_whole_final_commit_and_stale_cursor_reconciles.surql"),
            {"tasks": [task_record(task.task_id) for task in tasks]},
        )
        history = await runtime.live_updates_after(start)
        try:
            first = await anext(history)
            assert first.snapshot.progress == 0.25
        finally:
            await history.aclose()
        resumed = await runtime.live_updates_after(first.cursor)
        try:
            replayed = [await anext(resumed), await anext(resumed)]
            assert {update.snapshot.task_id for update in replayed} == {task.task_id for task in tasks}
            assert all(update.cursor == first.cursor for update in replayed)
        finally:
            await resumed.aclose()
        renewed = await runtime.live_updates_after(TaskUpdateCursor(0))
        try:
            assert { (await anext(renewed)).snapshot.task_id for _ in tasks } == {task.task_id for task in tasks}
        finally:
            await renewed.aclose()


async def test_idle_subscription_issues_no_queries_and_observes_another_replica(runtime, surreal_platform, monkeypatch):
    async with asyncio.timeout(15):
        caller = owner()
        task = (await runtime.create(draft(server=runtime.server, owner=caller))).snapshot
        subscription = await runtime.for_owner(caller).subscribe([task.task_id])
        await anext(subscription.updates)
        sleeping = asyncio.Event()
        original_wait = subscription.updates._wake.wait
        original_query = runtime.store.query
        queries = 0

        async def wait(timeout_seconds=None):
            sleeping.set()
            await original_wait(timeout_seconds)

        async def query(sql, variables=None):
            nonlocal queries
            queries += 1
            return await original_query(sql, variables)

        monkeypatch.setattr(subscription.updates._wake, "wait", wait)
        monkeypatch.setattr(runtime.store, "query", query)
        pending = asyncio.create_task(anext(subscription.updates))
        config = surreal_platform
        writer = await TaskRuntime.connect(
            config["endpoint"], config["namespace"], config["database"],
            config["username"], config["password"], runtime.server, "independent-worker",
        )
        try:
            await sleeping.wait()
            idle_count = queries
            await asyncio.sleep(2.1)
            assert queries == idle_count
            assert not pending.done()
            await writer.claim(str(task.task_id), timedelta(seconds=30))
            update = await pending
            assert update.snapshot.status == TaskStatus.RUNNING
            terminal = asyncio.create_task(runtime.await_terminal(str(task.task_id)))
            await writer.transition(str(task.task_id), TaskTransition.succeeded("done", None, result_uri=None))
            assert (await terminal).status == TaskStatus.SUCCEEDED
        finally:
            pending.cancel()
            await asyncio.gather(pending, return_exceptions=True)
            await subscription.updates.aclose()
            await writer.store.close()


async def test_socket_loss_closes_public_reader_and_new_subscription_reads_current_state(runtime):
    async with asyncio.timeout(15):
        caller = owner()
        task = (await runtime.create(draft(server=runtime.server, owner=caller))).snapshot
        query = runtime.for_owner(caller)
        subscription = await query.subscribe([task.task_id])
        await anext(subscription.updates)
        # Complete the source before iteration resumes: wake ownership must keep
        # the original receive task even after SDK.close clears its reference.
        await runtime.store.connection.close()
        with pytest.raises(StoreError, match="connection ended"):
            await anext(subscription.updates)
        assert subscription.updates._wake._closed
        renewed = await query.subscribe([task.task_id])
        try:
            assert (await anext(renewed.updates)).snapshot.task_id == task.task_id
        finally:
            await renewed.updates.aclose()


async def test_native_delete_keeps_typed_task_identity(runtime):
    async with asyncio.timeout(15):
        task = (await runtime.create(draft(server=runtime.server))).snapshot
        start = await changefeed_head(runtime.store)
        await runtime.store.query(test_query("test_native_changefeed/test_native_delete_keeps_typed_task_identity.surql"), {"task": task_record(task.task_id)})
        changes = [change for batch in await replay_changes(runtime.store, start) for change in batch.changes]
        deletion, = [change for change in changes if change.task_id() == task.task_id]
        assert deletion.current is None
        assert deletion.record == task_record(task.task_id)
        assert NativeChange.decode({"define_table": {"name": "task"}}) is None
        assert NativeChange.decode({"update": {"id": RecordID("other", "key")}}).task_id() is None
