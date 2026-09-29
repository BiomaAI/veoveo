"""SQL admission, current-state notification and transactional owner-race checks."""

import asyncio
import uuid
from dataclasses import replace
from datetime import datetime, timedelta, timezone

import pytest

from veoveo_mcp.tasks import (
    StoreError, TaskError, TaskInputRequest, TaskNotFound, TaskStatus,
    TaskTransition, TaskTypeName, TaskRuntime,
)
from veoveo_mcp.tasks.types import task_record
from test_task_runtime_integration import draft, owner


@pytest.fixture
async def runtime(surreal_platform):
    config = surreal_platform
    instance = await TaskRuntime.connect(
        config["endpoint"], config["namespace"], config["database"],
        config["username"], config["password"], f"query-{uuid.uuid4()}", "query-worker",
    )
    try:
        yield instance
    finally:
        from veoveo_mcp.tasks.types import server_record
        try:
            await instance.store.query(
                "LET $ids = SELECT VALUE <string> record::id(id) FROM task WHERE server = $server; "
                "DELETE outbox_event WHERE aggregate_type = 'task' AND aggregate_id IN $ids; "
                "DELETE domain_usage WHERE task.server = $server; "
                "DELETE task_input WHERE task.server = $server; DELETE task WHERE server = $server;",
                {"server": server_record(instance.server)},
            )
        finally:
            await instance.store.close()


@pytest.mark.parametrize("assignment", [
    "request.owner.data_labels = ['restricted']",
    "request.owner.profile = 'different'",
    "request.owner.principal_key = 'different'",
    "request.owner.tenant_key = 'different'",
    "task_type = 'other'",
])
async def test_owner_reads_and_subscription_admission_exclude_malformed_rows(runtime, assignment):
    async with asyncio.timeout(15):
        caller = owner(f"query-{uuid.uuid4()}")
        good = (await runtime.create(draft(server=runtime.server, owner=caller))).snapshot
        denied = (await runtime.create(draft(server=runtime.server, owner=caller))).snapshot
        await runtime.store.query(
            f"UPDATE $task SET {assignment}, request.owner.authority = {{}};",
            {"task": task_record(denied.task_id)},
        )
        query = runtime.for_owner(caller).of_type(TaskTypeName("profile"))
        assert await query.get(denied.task_id) is None
        assert (await query.get(good.task_id)).task_id == good.task_id
        page = await query.page(limit=1)
        assert [task.task_id for task in page.items] == [good.task_id]
        assert page.next_cursor is None
        subscription = await query.subscribe([denied.task_id, good.task_id])
        try:
            assert subscription.accepted_task_ids == (good.task_id,)
            assert (await anext(subscription.updates)).snapshot.task_id == good.task_id
        finally:
            await subscription.updates.aclose()
        with pytest.raises(TaskNotFound):
            await query.cancel(denied.task_id)


@pytest.mark.parametrize("assignment", [
    "work_context = work_context:wrong",
    "authority.context_key = 'wrong'",
    "request.owner.authority.work_context = 'wrong'",
    "request.owner.authority.tenant = 'wrong'",
])
async def test_work_context_checks_all_indexed_and_retained_coordinates(runtime, assignment):
    async with asyncio.timeout(15):
        caller = owner(f"context-{uuid.uuid4()}")
        snapshot = (await runtime.create(draft(server=runtime.server, owner=caller))).snapshot
        await runtime.store.query(
            f"UPDATE $task SET {assignment}, request.input = NONE;",
            {"task": task_record(snapshot.task_id)},
        )
        query = runtime.for_owner(caller).in_work_context()
        assert await query.get(snapshot.task_id) is None
        assert (await query.page()).items == ()
        subscription = await query.subscribe([snapshot.task_id])
        assert subscription.accepted_task_ids == ()
        await subscription.updates.aclose()


async def test_sql_pages_apply_clearance_before_limit_and_recheck_on_continuation(runtime):
    async with asyncio.timeout(15):
        caller = owner(f"pages-{uuid.uuid4()}")
        expected = []
        for labels in [frozenset({"restricted"}), frozenset()] * 4:
            task = (await runtime.create(draft(server=runtime.server, owner=replace(caller, data_labels=labels)))).snapshot
            if not labels:
                expected.append(task.task_id)
        same_time = datetime.now(timezone.utc)
        await runtime.store.query(
            "UPDATE task SET created_at = $now WHERE owner = $owner;",
            {"now": same_time, "owner": caller.principal_record()},
        )
        query = runtime.for_owner(caller)
        first = await query.page(limit=2)
        assert first.next_cursor is not None
        second = await query.page(first.next_cursor, limit=2)
        assert second.next_cursor is None
        assert [row.task_id for row in first.items + second.items] == sorted(expected)
        await runtime.store.query(
            "UPDATE $task SET request.owner.data_labels = ['restricted'], "
            "request.owner.authority = {};", {"task": task_record(second.items[0].task_id)},
        )
        remaining = await query.page(first.next_cursor, limit=2)
        assert [row.task_id for row in remaining.items] == [second.items[1].task_id]


async def test_notifications_read_current_state_and_ignore_malformed_event_snapshots(runtime):
    async with asyncio.timeout(15):
        caller = owner(f"stream-{uuid.uuid4()}")
        a, b = [(await runtime.create(draft(server=runtime.server, owner=caller))).snapshot for _ in range(2)]
        query = runtime.for_owner(caller)
        subscription = await query.subscribe([a.task_id, b.task_id])
        try:
            await anext(subscription.updates)
            await anext(subscription.updates)
            for snapshot in (a, b):
                await runtime.claim(str(snapshot.task_id), timedelta(seconds=30))
                await runtime.transition(str(snapshot.task_id), TaskTransition.succeeded("done", {"ok": True}))
            await runtime.store.query(
                "UPDATE $task SET request.owner.data_labels = ['restricted'], "
                "request.owner.authority = {}; "
                "UPDATE outbox_event SET payload.snapshot = {} "
                "WHERE aggregate_id IN $ids;",
                {"task": task_record(a.task_id), "ids": [str(a.task_id), str(b.task_id)]},
            )
            update = await anext(subscription.updates)
            assert update.snapshot.task_id == b.task_id
            assert update.snapshot.status == TaskStatus.SUCCEEDED
            assert update.snapshot.result.payload == {"ok": True}
        finally:
            await subscription.updates.aclose()
        assert subscription.updates._wake._closed


async def test_retained_event_gap_reconciles_and_new_subscription_uses_current_baseline(runtime):
    async with asyncio.timeout(15):
        caller = owner(f"gap-{uuid.uuid4()}")
        task = (await runtime.create(draft(server=runtime.server, owner=caller))).snapshot
        query = runtime.for_owner(caller)
        subscription = await query.subscribe([task.task_id])
        try:
            await anext(subscription.updates)
            await runtime.claim(str(task.task_id), timedelta(seconds=30))
            await runtime.transition(str(task.task_id), TaskTransition.succeeded("done", None))
            await runtime.store.query("DELETE outbox_event WHERE aggregate_id = $id;", {"id": str(task.task_id)})
            subscription.updates._reconcile_at = 0
            assert (await anext(subscription.updates)).snapshot.status == TaskStatus.SUCCEEDED
        finally:
            await subscription.updates.aclose()
        renewed = await query.subscribe([task.task_id])
        try:
            assert (await anext(renewed.updates)).snapshot.status == TaskStatus.SUCCEEDED
        finally:
            await renewed.updates.aclose()


async def test_subscription_close_before_iteration_and_cancellation_release_live_reader(runtime):
    async with asyncio.timeout(15):
        caller = owner(f"close-{uuid.uuid4()}")
        task = (await runtime.create(draft(server=runtime.server, owner=caller))).snapshot
        query = runtime.for_owner(caller)
        unstarted = await query.subscribe([task.task_id])
        await unstarted.updates.aclose()
        assert unstarted.updates._wake._closed
        active = await query.subscribe([task.task_id])
        await anext(active.updates)
        pending = asyncio.create_task(anext(active.updates))
        await asyncio.sleep(0.01)
        pending.cancel()
        with pytest.raises(asyncio.CancelledError):
            await pending
        assert active.updates._wake._closed


@pytest.mark.parametrize("action", ["cancel", "input"])
async def test_owner_revocation_inside_mutation_rolls_back_changes_and_events(runtime, monkeypatch, action):
    async with asyncio.timeout(15):
        caller = owner(f"mutation-{uuid.uuid4()}")
        task = (await runtime.create(draft(server=runtime.server, owner=caller))).snapshot
        if action == "input":
            await runtime.claim(str(task.task_id), timedelta(seconds=30))
            await runtime.request_input(str(task.task_id), "approval", TaskInputRequest(
                "elicitation/create", {"message": "Approve?", "requestedSchema": {"type": "object"}},
            ))
        query = runtime.for_owner(caller)
        before = await runtime.store.latest_available_outbox_sequence()
        original = runtime.store.query
        revoked = False

        async def revoke_before_write(sql, variables=None):
            nonlocal revoked
            if sql.startswith("BEGIN TRANSACTION; LET $updated") and not revoked:
                revoked = True
                await original("UPDATE $task SET request.owner.data_labels = ['restricted'];",
                               {"task": task_record(task.task_id)})
            return await original(sql, variables)

        monkeypatch.setattr(runtime.store, "query", revoke_before_write)
        if action == "cancel":
            with pytest.raises(TaskNotFound):
                await query.cancel(task.task_id)
            assert (await runtime.get(str(task.task_id))).status == TaskStatus.QUEUED
        else:
            with pytest.raises(StoreError, match="cannot accept input"):
                await query.submit_input_responses(task.task_id, {"approval": {"action": "accept"}})
            assert "approval" in await runtime.outstanding_inputs(str(task.task_id))
            assert await query.outstanding_inputs(task.task_id) == {}
        assert revoked
        assert await runtime.store.latest_available_outbox_sequence() == before


async def test_query_admission_rejects_untyped_ids_and_unbounded_selections(runtime):
    query = runtime.for_owner(owner())
    for kinds in [[], [TaskTypeName("profile")] * 33, ["profile"]]:
        with pytest.raises(TaskError):
            query.of_types(kinds)
    for limit in [0, -1, 1001, True]:
        with pytest.raises(TaskError):
            await query.page(limit=limit)
    with pytest.raises(TaskError):
        await query.get(str(uuid.uuid4()))
    with pytest.raises(TaskError):
        await query.subscribe([uuid.uuid4()] * 257)
    for name in ["", "Profile", "a b", "a" * 129]:
        with pytest.raises(TaskError):
            TaskTypeName(name)
