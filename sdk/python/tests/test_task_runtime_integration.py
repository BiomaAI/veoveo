"""Live SurrealDB integration tests for the Python task runtime port.

These mirror the behavior guaranteed by the Rust `veoveo-task-runtime` crate:
durable creation with atomic outbox events, leases, CAS transitions,
cancellation, idempotency, input exchange, recovery, and pruning.
"""

import asyncio
import json
import uuid
from dataclasses import replace
from datetime import datetime, timedelta, timezone

import pytest
from pydantic import ValidationError
from surrealdb import RecordID

from veoveo_mcp.contract import InvocationAuthority
from veoveo_mcp.tasks import (
    Conflict,
    CreateTask,
    InvalidRecord,
    InvalidTransition,
    LeaseHeld,
    PrincipalKind,
    RecoveryClass,
    TaskFailure,
    TaskInputRequest,
    TaskOwner,
    TaskResult,
    TaskRuntime,
    TaskSnapshot,
    TaskStatus,
    TaskTransition,
    new_task_id,
)
from veoveo_mcp.task_extension.projection import project_snapshot
from veoveo_mcp.tasks.runtime import _task_snapshot_from_event
from veoveo_mcp.tasks.types import profile_record, server_record, task_record

SERVER = "datasheet"


def owner(principal: str = "conformance") -> TaskOwner:
    principal_id = f"https://conformance.veoveo.local#{principal}"
    return TaskOwner(
        principal_key=principal_id,
        principal_kind=PrincipalKind.SERVICE,
        issuer="https://conformance.veoveo.local",
        subject=principal,
        profile="operator",
        tenant_key="local",
        authority=InvocationAuthority.model_validate(
            {
                "work_context": "operations",
                "tenant": "local",
                "membership": "contributor",
                "policy_revision": "r1",
                "output_policy": {
                    "owner": {"kind": "group", "id": "operations"},
                    "initial_grants": [
                        {
                            "subject": {"kind": "group", "id": "operations"},
                            "level": "read",
                        }
                    ],
                },
                "provenance": {"mode": "automated"},
            }
        ),
        data_labels=frozenset(),
    )


@pytest.fixture
async def runtime(surreal_platform):
    runtime = await TaskRuntime.connect(
        surreal_platform["endpoint"],
        surreal_platform["namespace"],
        surreal_platform["database"],
        surreal_platform["username"],
        surreal_platform["password"],
        SERVER,
        f"{SERVER}-{uuid.uuid4()}",
    )
    yield runtime
    await runtime.store.close()


def draft(**overrides) -> CreateTask:
    values = dict(
        task_id=new_task_id(),
        owner=owner(),
        server=SERVER,
        task_type="profile",
        request={"dataset": "inline", "n": 1},
        recovery_class=RecoveryClass.RESUME,
        idempotency_key=None,
        ttl_ms=60_000,
        poll_interval_ms=500,
        retention_pins=frozenset(),
    )
    values.update(overrides)
    return CreateTask(**values)


async def test_create_claim_transition_succeed_roundtrip(runtime):
    created = await runtime.create(draft(retention_pins=frozenset(["agent-episode:1"])))
    assert created.created
    snapshot = created.snapshot
    assert snapshot.status == TaskStatus.QUEUED
    assert snapshot.status_message == "Queued"
    assert snapshot.retention_pins == frozenset(["agent-episode:1"])
    assert snapshot.ttl_ms == 60_000

    claimed = await runtime.claim(str(snapshot.task_id), timedelta(seconds=30))
    assert claimed.snapshot.status == TaskStatus.RUNNING
    assert claimed.snapshot.lease_owner == runtime.worker_id

    running = await runtime.transition(
        str(snapshot.task_id), TaskTransition.running("halfway", 0.5)
    )
    assert running.progress == 0.5
    assert running.status_message == "halfway"

    done = await runtime.transition(
        str(snapshot.task_id),
        TaskTransition.succeeded("done", {"content": [], "isError": False}),
    )
    assert done.status == TaskStatus.SUCCEEDED
    assert done.progress == 1.0
    assert done.lease_owner is None
    assert done.result is not None
    assert done.result.payload == {"content": [], "isError": False}
    assert done.completed_at is not None

    with pytest.raises(InvalidTransition):
        await runtime.transition(
            str(snapshot.task_id), TaskTransition.running("again", 0.1)
        )

    pinned = await runtime.acknowledge_retention_pin(
        str(snapshot.task_id), "agent-episode:1"
    )
    assert pinned.retention_pins == frozenset()


async def test_outbox_events_stream_snapshots(runtime):
    updates = await runtime.live_updates()
    created = await runtime.create(draft())
    task_id = str(created.snapshot.task_id)
    await runtime.claim(task_id, timedelta(seconds=30))
    await runtime.transition(task_id, TaskTransition.succeeded("ok", {"value": 1}))

    seen: list[TaskStatus] = []

    async def watch():
        async for update in updates:
            if str(update.snapshot.task_id) != task_id:
                continue
            seen.append(update.snapshot.status)
            if update.snapshot.status == TaskStatus.SUCCEEDED:
                return

    await asyncio.wait_for(watch(), timeout=15)
    assert TaskStatus.QUEUED in seen
    assert TaskStatus.RUNNING in seen
    assert seen[-1] == TaskStatus.SUCCEEDED


@pytest.mark.parametrize(
    "payload",
    [
        None,
        {"value": 42},
        {"payload": None},
        [None, {"x": None}],
        [],
        42,
        False,
        {"bounds": [-2**63, 2**63 - 1, 2**63, 2**64 - 1], "fraction": 2.5, "null": None},
    ],
)
async def test_results_preserve_json_shape_through_store_replay_and_mcp(
    runtime, surreal_platform, payload
):
    async with asyncio.timeout(15):
        start = await runtime.store.latest_available_outbox_sequence()
        created = (await runtime.create(draft())).snapshot
        task_id = str(created.task_id)
        assert created.result is None
        assert "result" not in created.to_json()
        assert TaskSnapshot.from_json(created.to_json()).result is None
        await runtime.claim(task_id, timedelta(seconds=30))
        done = await runtime.transition(task_id, TaskTransition.succeeded("done", payload))
        expected = TaskResult(payload)
        assert done.result == expected
        assert done.to_json()["result"] == payload
        assert json.loads(json.dumps(done.to_json()))["result"] == payload
        assert TaskSnapshot.from_json(done.to_json()).result == expected
        observer = await TaskRuntime.connect(
            surreal_platform["endpoint"],
            surreal_platform["namespace"],
            surreal_platform["database"],
            surreal_platform["username"],
            surreal_platform["password"],
            SERVER,
            f"observer-{uuid.uuid4()}",
        )
        try:
            assert (await observer.get(task_id)).result == expected
        finally:
            await observer.store.close()
        stored = await runtime.store.connection.select(task_record(created.task_id))
        assert len(stored) == 1
        assert stored[0]["result"] == {"payload": payload}

        events = [
            event
            for event in await runtime.store.read_outbox(start, 1000)
            if event.aggregate_type == "task" and event.aggregate_id == task_id
        ]
        assert len(events) == 3
        assert all(event.schema_version == 3 for event in events)
        assert "result" not in events[0].payload["snapshot"]
        assert events[-1].payload["snapshot"]["result"] == payload
        replay = _task_snapshot_from_event(events[-1])
        assert replay.result == expected
        projected = await project_snapshot(runtime, replay)
        expected_protocol = payload if isinstance(payload, dict) else {"value": payload}
        assert projected.result == expected_protocol
        assert projected.model_dump(mode="json")["result"] == projected.result
        with pytest.raises(InvalidRecord, match="schema version"):
            _task_snapshot_from_event(replace(events[-1], schema_version=2))


async def test_idempotent_create_returns_existing(runtime):
    key = f"request-{uuid.uuid4()}"
    first = await runtime.create(draft(idempotency_key=key))
    second = await runtime.create(draft(idempotency_key=key))
    assert first.created
    assert not second.created
    assert second.snapshot.task_id == first.snapshot.task_id


async def test_lease_is_exclusive_and_cas_conflicts(runtime, surreal_platform):
    created = await runtime.create(draft())
    task_id = str(created.snapshot.task_id)
    await runtime.claim(task_id, timedelta(seconds=30))

    other = await TaskRuntime.connect(
        surreal_platform["endpoint"],
        surreal_platform["namespace"],
        surreal_platform["database"],
        surreal_platform["username"],
        surreal_platform["password"],
        SERVER,
        f"{SERVER}-other-{uuid.uuid4()}",
    )
    try:
        with pytest.raises(LeaseHeld):
            await other.claim(task_id, timedelta(seconds=30))
        stale = created.snapshot
        with pytest.raises(Conflict):
            await runtime.transition_if_current(
                stale, TaskTransition.running("stale", 0.2)
            )
    finally:
        await other.store.close()


async def test_cancel_requested_then_cancelled(runtime):
    created = await runtime.create(draft())
    task_id = str(created.snapshot.task_id)
    cancelled = await runtime.cancel(task_id)
    assert cancelled.status == TaskStatus.CANCELLED

    claimed_draft = await runtime.create(draft())
    claimed_id = str(claimed_draft.snapshot.task_id)
    await runtime.claim(claimed_id, timedelta(seconds=30))
    requested = await runtime.cancel(claimed_id)
    assert requested.status == TaskStatus.CANCEL_REQUESTED
    assert await runtime.is_cancel_requested(claimed_id)
    finished = await runtime.transition(claimed_id, TaskTransition.cancelled())
    assert finished.status == TaskStatus.CANCELLED


async def test_input_exchange_roundtrip(runtime):
    created = await runtime.create(draft())
    task_id = str(created.snapshot.task_id)

    with pytest.raises(LeaseHeld):
        await runtime.request_input(
            task_id, "confirm", TaskInputRequest(method="elicitation/create")
        )

    await runtime.claim(task_id, timedelta(seconds=30))
    exchange = await runtime.request_input(
        task_id,
        "confirm",
        TaskInputRequest(method="elicitation/create", params={"prompt": "go on?"}),
    )
    assert exchange.key == "confirm"
    assert exchange.response is None

    outstanding = await runtime.outstanding_inputs(task_id)
    assert list(outstanding) == ["confirm"]

    submission = await runtime.submit_input_responses(
        task_id, {"confirm": {"approved": True}, "unknown": {"x": 1}}
    )
    assert submission.accepted == 1
    assert submission.ignored == 1
    assert await runtime.outstanding_inputs(task_id) == {}


async def test_recovery_resets_expired_resume_leases(runtime):
    created = await runtime.create(draft())
    task_id = str(created.snapshot.task_id)
    await runtime.claim(task_id, timedelta(milliseconds=50))
    await asyncio.sleep(0.2)

    report = await runtime.recover()
    recovered = {str(snapshot.task_id) for snapshot in report.resumable}
    assert task_id in recovered
    snapshot = await runtime.get(task_id)
    assert snapshot is not None and snapshot.status == TaskStatus.QUEUED


async def test_recovery_fails_interrupted_indeterminate(runtime):
    created = await runtime.create(
        draft(recovery_class=RecoveryClass.INTERRUPTED_INDETERMINATE)
    )
    task_id = str(created.snapshot.task_id)
    await runtime.claim(task_id, timedelta(milliseconds=50))
    await asyncio.sleep(0.2)

    report = await runtime.recover()
    failed = {str(snapshot.task_id) for snapshot in report.failed_indeterminate}
    assert task_id in failed
    snapshot = await runtime.get(task_id)
    assert snapshot is not None
    assert snapshot.status == TaskStatus.FAILED
    assert snapshot.error is not None
    assert snapshot.error.code == "interrupted_indeterminate"


async def test_prune_removes_expired_unpinned_terminal_tasks(runtime):
    created = await runtime.create(draft(ttl_ms=1))
    task_id = str(created.snapshot.task_id)
    await runtime.claim(task_id, timedelta(seconds=30))
    await runtime.transition(
        task_id, TaskTransition.failed(TaskFailure("boom", "exploded"))
    )
    await asyncio.sleep(0.05)
    pruned = await runtime.prune_expired()
    assert created.snapshot.task_id in pruned
    assert await runtime.get(task_id) is None


async def test_domain_usage_rows_are_recorded_and_queryable(runtime):
    created = await runtime.create(draft())
    task_id = created.snapshot.task_id
    await runtime.store.upsert_domain_usage(
        task_id=task_id,
        server=SERVER,
        model_id="datasheet/profile",
        kind="actual",
        quantity=3.0,
        unit="column",
        recorded_at=datetime.now(timezone.utc),
    )
    rows = await runtime.for_owner(owner()).usage().get(task_id)
    assert len(rows) == 1
    assert rows[0].model_id == "datasheet/profile"
    ids = (await runtime.for_owner(owner()).usage().page(limit=1000)).task_ids
    assert task_id in ids


async def test_snapshot_json_matches_rust_serde_shape(runtime):
    created = await runtime.create(draft())
    payload = created.snapshot.to_json()
    assert set(payload) == {
        "task_id",
        "owner",
        "server",
        "task_type",
        "request",
        "recovery_class",
        "status",
        "status_message",
        "progress",
        "error",
        "idempotency_key",
        "lease_owner",
        "lease_expires_at",
        "cancel_requested_at",
        "created_at",
        "updated_at",
        "started_at",
        "completed_at",
        "retention_expires_at",
        "retention_pins",
        "ttl_ms",
        "poll_interval_ms",
    }
    assert payload["status"] == "queued"
    assert payload["recovery_class"] == "resume"
    assert payload["owner"]["principal_kind"] == "service"
    assert payload["created_at"].endswith("Z")


@pytest.mark.parametrize(
    "assignment",
    [
        "owner = $different_owner",
        "tenant = $different_tenant",
        "profile = $different_profile",
        "server = $different_server",
        "request.owner.principal_key = 'someone-else'",
        "request.owner.profile = 'other-profile'",
        "request.owner.tenant_key = 'other-tenant'",
        "request.owner.data_labels = ['restricted']",
        "request.owner.data_labels = NONE",
        "request.owner.data_labels = 'restricted'",
        "request.owner = {}",
    ],
)
async def test_owner_sql_excludes_denied_malformed_rows(runtime, assignment):
    async with asyncio.timeout(15):
        caller = owner(f"sql-owner-{uuid.uuid4()}")
        created = (await runtime.create(draft(owner=caller))).snapshot
        other = owner(f"other-{uuid.uuid4()}")
        await runtime.store.query(
            f"UPDATE $task SET {assignment}, request.owner.authority = {{}};",
            {
                "task": task_record(created.task_id),
                "different_owner": other.principal_record(),
                "different_tenant": RecordID("tenant", uuid.uuid4()),
                "different_profile": profile_record("other-profile"),
                "different_server": server_record("other-server"),
            },
        )
        assert (await runtime.for_owner(caller).page(limit=1000)).items == ()
        # The denied body really is malformed; selection must precede decoding.
        raw = await runtime.store.connection.select(task_record(created.task_id))
        assert raw[0]["request"]["owner"]["authority"] == {}


async def test_owner_sql_rechecks_clearance_through_independent_connection(
    runtime, surreal_platform
):
    async with asyncio.timeout(15):
        caller = replace(
            owner(f"clearance-{uuid.uuid4()}"), data_labels=frozenset({"a", "b"})
        )
        tasks = {}
        for labels in [
            frozenset(), frozenset({"a"}), frozenset({"b"}), frozenset({"a", "b"})
        ]:
            created = await runtime.create(draft(owner=replace(caller, data_labels=labels)))
            tasks[labels] = created.snapshot.task_id
        observer = await TaskRuntime.connect(
            surreal_platform["endpoint"],
            surreal_platform["namespace"],
            surreal_platform["database"],
            surreal_platform["username"],
            surreal_platform["password"],
            SERVER,
            f"reader-{uuid.uuid4()}",
        )
        try:
            for clearance in [frozenset({"a"}), frozenset(), frozenset({"a", "b"})]:
                rows = (await observer.for_owner(replace(caller, data_labels=clearance)).page(limit=1000)).items
                assert {row.task_id for row in rows} == {
                    task for labels, task in tasks.items() if labels.issubset(clearance)
                }
            changed = tasks[frozenset({"a"})]
            await runtime.store.query(
                "UPDATE $task SET request.owner.data_labels = ['restricted'];",
                {"task": task_record(changed)},
            )
            current = (await observer.for_owner(caller).page(limit=1000)).items
            assert changed not in {row.task_id for row in current}
            await runtime.store.query(
                "UPDATE $task SET request.owner.authority = {};",
                {"task": task_record(changed)},
            )
            # A revoked row stays outside decoding; an admitted malformed row fails.
            current = (await observer.for_owner(caller).page(limit=1000)).items
            assert changed not in {row.task_id for row in current}
            with pytest.raises(ValidationError):
                await observer.for_owner(
                    replace(caller, data_labels=caller.data_labels | {"restricted"})
                ).page(limit=1000)
        finally:
            await observer.store.close()


async def test_owner_sql_distinguishes_absent_and_named_installation_tenants(runtime):
    async with asyncio.timeout(15):
        seed = owner(f"optional-tenant-{uuid.uuid4()}")
        authority = seed.authority.model_copy(update={"tenant": "installation"})
        absent = replace(seed, tenant_key=None, authority=authority)
        named = replace(seed, tenant_key="installation", authority=authority)
        absent_task = (await runtime.create(draft(owner=absent))).snapshot.task_id
        named_task = (await runtime.create(draft(owner=named))).snapshot.task_id
        assert {row.task_id for row in (await runtime.for_owner(absent).page(limit=1000)).items} == {absent_task}
        assert {row.task_id for row in (await runtime.for_owner(named).page(limit=1000)).items} == {named_task}


async def test_trusted_get_rejects_foreign_server_before_decoding(runtime):
    async with asyncio.timeout(15):
        created = (await runtime.create(draft())).snapshot
        await runtime.store.query(
            "UPDATE $task SET server = $server, request.owner.authority = {};",
            {"task": task_record(created.task_id), "server": server_record("other-server")},
        )
        assert await runtime.get(str(created.task_id)) is None
        foreign = TaskRuntime(runtime.store, "other-server", "read-only-observer")
        with pytest.raises(ValidationError):
            await foreign.get(str(created.task_id))


async def test_live_wake_survives_idle_deadlines_and_closes_on_cancel(
    runtime, surreal_platform
):
    writer = await TaskRuntime.connect(
        surreal_platform["endpoint"],
        surreal_platform["namespace"],
        surreal_platform["database"],
        surreal_platform["username"],
        surreal_platform["password"],
        SERVER,
        "independent-live-writer",
    )
    wake = None
    pending = None
    try:
        async with asyncio.timeout(15):
            wake = await runtime.store.outbox_wake()
            for _ in range(3):
                await wake.wait(0.01)
            for _ in range(2):
                pending = asyncio.create_task(wake.wait(5))
                await asyncio.sleep(0.01)
                assert not pending.done(), "idle deadline closed the LIVE reader"
                await writer.create(draft(owner=owner("idle-live-writer")))
                await asyncio.wait_for(pending, 2)
                pending = None
                await wake.wait(0.01)

            async def consume():
                try:
                    await wake.wait(5)
                finally:
                    await wake.close()

            pending = asyncio.create_task(consume())
            await asyncio.sleep(0.01)
            assert not pending.done()
            pending.cancel()
            with pytest.raises(asyncio.CancelledError):
                await asyncio.wait_for(pending, 2)
            pending = None
            assert not runtime.store._db.live_queues
    finally:
        if pending is not None:
            pending.cancel()
            await asyncio.wait_for(
                asyncio.gather(pending, return_exceptions=True), 2
            )
        if wake is not None:
            await asyncio.wait_for(wake.close(), 11)
        await asyncio.wait_for(writer.store.close(), 5)
