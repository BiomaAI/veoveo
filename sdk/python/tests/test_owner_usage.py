import asyncio
import uuid
from dataclasses import replace

import pytest

from veoveo_mcp.tasks import TaskTypeName
from veoveo_mcp.tasks.types import task_record
from test_owner_task_query import runtime  # noqa: F401
from test_task_runtime_integration import draft, owner


async def record_usage(runtime, caller, **overrides):
    task = (await runtime.create(draft(server=runtime.server, owner=caller, **overrides))).snapshot
    await runtime.store.upsert_domain_usage(task.task_id, runtime.server, "profile", "actual", amount=1.0, currency="USD")
    return task.task_id


async def test_usage_pages_group_after_owner_admission_and_complete_in_sql(runtime):
    async with asyncio.timeout(60):
        caller = owner(f"usage-{uuid.uuid4()}")
        for _ in range(15):
            await record_usage(runtime, replace(caller, data_labels=frozenset({"restricted"})))
        expected = [await record_usage(runtime, caller) for _ in range(103)]
        # Multiple usage records must not consume another collection slot.
        await runtime.store.upsert_domain_usage(expected[0], runtime.server, "other-model", "actual")
        query = runtime.for_owner(caller).of_type(TaskTypeName("profile")).usage()
        first = await query.page()
        assert len(first.task_ids) == 100
        second = await query.page(first.next_task_id)
        assert second.next_task_id is None
        assert first.task_ids + second.task_ids == tuple(sorted(expected))
        complete = await query.complete("")
        assert complete.task_ids == first.task_ids
        assert complete.has_more
        exact = await query.complete(str(expected[-1]))
        assert exact.task_ids == (expected[-1],)
        assert not exact.has_more
        assert (await query.complete("' OR true; --")).task_ids == ()
        records = await query.get(expected[0])
        assert {record.model_id for record in records} == {"profile", "other-model"}


@pytest.mark.parametrize("target,assignment", [
    ("task", "request.owner.data_labels = ['restricted']"),
    ("task", "profile = profile:other"),
    ("task", "request.owner.principal_key = 'other'"),
    ("task", "task_type = 'other'"),
    ("usage", "server = mcp_server:other"),
    ("usage", "tenant = tenant:other"),
])
async def test_usage_point_page_and_completion_apply_current_parent_metadata(runtime, target, assignment):
    async with asyncio.timeout(15):
        caller = owner(f"usage-denial-{uuid.uuid4()}")
        task_id = await record_usage(runtime, caller)
        query = runtime.for_owner(caller).of_type(TaskTypeName("profile")).usage()
        assert len(await query.get(task_id)) == 1
        table = "$task" if target == "task" else "domain_usage"
        where = "" if target == "task" else " WHERE task = $task"
        await runtime.store.query(f"UPDATE {table} SET {assignment}{where};",
                                  {"task": task_record(task_id)})
        await runtime.store.query("UPDATE $task SET request.owner.authority = {};",
                                  {"task": task_record(task_id)})
        assert await query.get(task_id) == ()
        assert (await query.page(limit=1)).task_ids == ()
        assert (await query.complete("")).task_ids == ()


async def test_usage_work_context_is_an_explicit_sql_selection(runtime):
    async with asyncio.timeout(15):
        caller = owner(f"usage-context-{uuid.uuid4()}")
        task_id = await record_usage(runtime, caller)
        other = replace(caller, authority=caller.authority.model_copy(update={"work_context": "other"}))
        assert len(await runtime.for_owner(other).usage().get(task_id)) == 1
        scoped = runtime.for_owner(other).in_work_context().usage()
        assert await scoped.get(task_id) == ()
        assert (await scoped.page()).task_ids == ()
        assert (await scoped.complete("")).task_ids == ()
