"""Live SurrealDB integration tests for the Python task runtime port.

These mirror the behavior guaranteed by the Rust `veoveo-task-runtime` crate:
durable creation with atomic outbox events, leases, CAS transitions,
cancellation, idempotency, input exchange, recovery, and pruning.
"""

import asyncio
import uuid
from datetime import datetime, timedelta, timezone

import pytest

from veoveo_mcp.tasks import (
    Conflict,
    CreateTask,
    InvalidTransition,
    LeaseHeld,
    PrincipalKind,
    RecoveryClass,
    TaskFailure,
    TaskInputRequest,
    TaskOwner,
    TaskRuntime,
    TaskStatus,
    TaskTransition,
    new_task_id,
)

SERVER = "datasheet"


def owner(principal: str = "conformance") -> TaskOwner:
    return TaskOwner(
        principal_key=principal,
        principal_kind=PrincipalKind.SERVICE,
        issuer="https://conformance.veoveo.local",
        subject=principal,
        profile="operator",
        tenant_key="local",
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
    assert snapshot.status_message == "accepted; queued"
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
    assert done.result == {"content": [], "isError": False}
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
    rows = await runtime.store.domain_usage_for_task(SERVER, task_id)
    assert len(rows) == 1
    assert rows[0]["model_id"] == "datasheet/profile"
    ids = await runtime.store.domain_usage_task_ids(SERVER)
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
        "result",
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
