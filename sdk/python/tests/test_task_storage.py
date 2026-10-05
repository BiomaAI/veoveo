"""Stored-owner closure, full identity admission and request/owner CAS races."""

import asyncio
from dataclasses import replace
from datetime import datetime, timedelta, timezone

import pytest
from pydantic import ValidationError
from surrealdb import RecordID

from query_files import test_query
from test_task_runtime_integration import draft, owner, runtime  # noqa: F401
from veoveo_mcp.tasks import Conflict, InvalidRecord, LeaseHeld, TaskError, TaskInputRequest, TaskTransition
from veoveo_mcp.tasks.records import OwnerContextRecord
from veoveo_mcp.tasks.runtime import (
    _authority_record, _initiator_record, _record_to_snapshot, _work_context_record,
)
from veoveo_mcp.tasks.store import StoreError
from veoveo_mcp.tasks.types import profile_record, server_record, task_record


@pytest.fixture
def stored_task():
    request = draft()
    caller = request.owner
    now = datetime.now(timezone.utc)
    return {
        "id": task_record(request.task_id), "tenant": caller.tenant_record(),
        "owner": caller.principal_record(), "profile": profile_record(caller.profile),
        "server": server_record(request.server), "work_context": _work_context_record(caller),
        "initiator": _initiator_record(caller), "invocation_mode": caller.authority.invocation_mode,
        "delegation_id": caller.authority.delegation_id, "policy_revision": caller.authority.policy_revision,
        "authority": _authority_record(caller), "owner_context": caller.to_json(),
        "request": {"input": request.request, "status_message": "Queued",
                    "ttl_ms": request.ttl_ms, "poll_interval_ms": request.poll_interval_ms},
        "task_type": request.task_type, "recovery_class": request.recovery_class.value,
        "status": "queued", "progress": 0.0, "created_at": now, "updated_at": now,
        "updated_at_exact": now.isoformat(), "created_at_exact": now.isoformat(),
    }


@pytest.mark.parametrize("column", ["id", "tenant", "owner", "profile", "work_context", "server"])
def test_decoder_rejects_same_key_in_wrong_record_table(stored_task, column):
    stored_task[column] = RecordID("wrong", stored_task[column].id)
    with pytest.raises(InvalidRecord):
        _record_to_snapshot(stored_task)


@pytest.mark.parametrize("path", [
    ("owner_context",), ("owner_context", "authority"),
    ("owner_context", "authority", "output_policy"),
    ("owner_context", "authority", "output_policy", "owner"),
    ("owner_context", "authority", "output_policy", "initial_grants", 0),
    ("owner_context", "authority", "output_policy", "initial_grants", 0, "subject"),
    ("owner_context", "authority", "provenance"), ("request",), ("authority",),
])
def test_decoder_rejects_unknown_nested_fields(stored_task, path):
    target = stored_task
    for segment in path:
        target = target[segment]
    target["unexpected"] = True
    with pytest.raises(ValidationError):
        _record_to_snapshot(stored_task)


@pytest.mark.parametrize("path,key", [
    ((), "owner_context"), (("owner_context",), "principal_key"), (("owner_context",), "data_labels"),
    (("owner_context",), "authority"), (("owner_context", "authority"), "tenant"),
    (("owner_context", "authority"), "provenance"), (("request",), "input"),
])
def test_decoder_rejects_missing_required_fields(stored_task, path, key):
    target = stored_task
    for segment in path:
        target = target[segment]
    del target[key]
    with pytest.raises((ValidationError, KeyError)):
        _record_to_snapshot(stored_task)


@pytest.mark.parametrize("column,value", [
    ("invocation_mode", "direct"), ("delegation_id", "different"),
    ("policy_revision", "different"), ("initiator", RecordID("principal", "different")),
])
def test_decoder_checks_indexed_authority_columns(stored_task, column, value):
    stored_task[column] = value
    with pytest.raises(InvalidRecord):
        _record_to_snapshot(stored_task)


def test_decoder_checks_full_authority_and_separates_clearance(stored_task):
    stored_task["owner_context"]["data_labels"] = ["clearance"]
    assert _record_to_snapshot(stored_task).owner.data_labels == {"clearance"}
    stored_task["authority"]["owner_key"] = "different"
    with pytest.raises(InvalidRecord):
        _record_to_snapshot(stored_task)


def test_owner_optional_installation_tenant_and_provenance_are_preserved():
    caller = owner()
    authority = caller.authority.model_copy(update={"tenant": "installation"})
    absent = replace(caller, tenant_key=None, authority=authority)
    named = replace(caller, tenant_key="installation", authority=authority)
    assert OwnerContextRecord.from_owner(absent).to_owner() == absent
    assert OwnerContextRecord.from_owner(named).to_owner() == named
    assert absent != named


@pytest.mark.parametrize("mutation", [
    "unknown_owner", "unknown_authority", "unknown_policy", "unknown_grant", "unknown_provenance",
    "unknown_request", "retired_request_owner", "unknown_subject", "missing_owner_key",
    "missing_owner_labels", "missing_authority", "missing_input", "invalid_labels",
])
async def test_native_closed_task_fields_reject_mutation_atomically(runtime, mutation):
    async with asyncio.timeout(15):
        created = (await runtime.create(draft())).snapshot
        before = await runtime.get(str(created.task_id))
        bindings = {"task": task_record(created.task_id)}
        if mutation in {"unknown_grant", "unknown_subject"}:
            grants = before.owner.authority.output_policy.model_dump(mode="json")["initial_grants"]
            assert grants and grants[0]["level"] == "read"
            indexed = test_query(f"test_task_storage/{mutation}_indexed.surql")
            try:
                await runtime.store.query(indexed, bindings)
            except StoreError:
                pass
            raw = await runtime.store.query(test_query("test_task_storage/read_grants.surql"), bindings)
            assert raw[0]["grants"] == grants, "indexed mutation changed the stored grant array"
            target = grants[0] if mutation == "unknown_grant" else grants[0]["subject"]
            target["unexpected"] = True
            bindings["grants"] = grants
        with pytest.raises(StoreError):
            await runtime.store.query(test_query(f"test_task_storage/{mutation}.surql"), bindings)
        assert await runtime.get(str(created.task_id)) == before


@pytest.mark.parametrize("payload", [
    None, [None, {"unknown": [1, {"arbitrary": True}]}],
    {"owner": {"authority": "provider data"}, "unexpected": None, "large": 2**64 - 1},
])
async def test_native_request_input_stays_opaque(runtime, payload):
    async with asyncio.timeout(15):
        created = (await runtime.create(draft(request=payload))).snapshot
        assert created.request == payload
        claimed = (await runtime.claim(str(created.task_id), timedelta(seconds=30))).snapshot
        assert claimed.request == payload
        result = await runtime.transition_if_current(claimed, TaskTransition.succeeded("Done", {"ok": True}, result_uri=None))
        assert result.request == payload


@pytest.mark.parametrize("operation", ["claim", "transition", "request_input", "recovery", "input_response"])
async def test_native_owner_change_without_timestamp_change_fails_cas(runtime, monkeypatch, operation):
    async with asyncio.timeout(15):
        created = (await runtime.create(draft())).snapshot
        if operation in ["transition", "request_input", "input_response"]:
            created = (await runtime.claim(str(created.task_id), timedelta(seconds=30))).snapshot
        if operation == "input_response":
            await runtime.request_input(str(created.task_id), "approval", TaskInputRequest("elicitation/create", {}))
            created = await runtime.get(str(created.task_id))
        original = runtime.store.query
        raced = False

        async def race(sql, bindings=None):
            nonlocal raced
            if "$expected_owner_context" in sql and not raced:
                raced = True
                await original(test_query("test_task_storage/change_clearance.surql"),
                               {"task": task_record(created.task_id)})
            return await original(sql, bindings)

        monkeypatch.setattr(runtime.store, "query", race)
        with pytest.raises((Conflict, LeaseHeld, StoreError)):
            if operation == "claim":
                await runtime.claim(str(created.task_id), timedelta(seconds=30))
            elif operation == "transition":
                await runtime.transition_if_current(created, TaskTransition.succeeded("Done", {"ok": True}, result_uri=None))
            elif operation == "request_input":
                await runtime.request_input(str(created.task_id), "blocked", TaskInputRequest("elicitation/create", {}))
            elif operation == "recovery":
                await runtime._force_waiting(created)
            else:
                await runtime.submit_input_responses(str(created.task_id), {"approval": {"ok": True}})
        assert raced
        current = await runtime.get(str(created.task_id))
        assert current.owner.data_labels == {"changed"}
        assert current.updated_at == created.updated_at
        assert current.status == created.status
        if operation == "request_input":
            assert "blocked" not in await runtime.outstanding_inputs(str(created.task_id))
        if operation == "input_response":
            assert "approval" in await runtime.outstanding_inputs(str(created.task_id))


@pytest.mark.parametrize("mode", ["direct", "delegated"])
def test_decoder_checks_full_initiator_identity(stored_task, mode):
    caller = stored_task["owner_context"]
    provenance = {"mode": mode, "initiator": caller["principal_key"]}
    if mode == "delegated":
        provenance["delegation_id"] = "delegation-1"
    caller["authority"]["provenance"] = provenance
    decoded = OwnerContextRecord.model_validate(caller).to_owner()
    stored_task.update({"authority": _authority_record(decoded), "initiator": _initiator_record(decoded),
                        "invocation_mode": mode, "delegation_id": decoded.authority.delegation_id})
    assert _record_to_snapshot(stored_task).owner == decoded
    stored_task["initiator"] = RecordID("wrong", stored_task["initiator"].id)
    with pytest.raises(InvalidRecord):
        _record_to_snapshot(stored_task)


@pytest.mark.parametrize("mode", ["direct", "delegated", "automated"])
async def test_native_owner_context_preserves_full_invocation_authority(runtime, mode):
    caller = owner()
    payload = caller.authority.model_dump(mode="json")
    payload["provenance"] = {"mode": mode}
    if mode != "automated":
        payload["provenance"]["initiator"] = caller.principal_key
    if mode == "delegated":
        payload["provenance"]["delegation_id"] = "delegation-1"
    caller = replace(caller, authority=type(caller.authority).model_validate(payload),
                     data_labels=frozenset({"clearance"}))
    created = (await runtime.create(draft(owner=caller))).snapshot
    assert created.owner == caller
    assert (await runtime.get(str(created.task_id))).owner == caller


@pytest.mark.parametrize("field", ["ttl_ms", "poll_interval_ms"])
@pytest.mark.parametrize("value", [-1, 2**64, True, 1.5])
def test_request_record_rejects_non_u64_metadata(field, value):
    from veoveo_mcp.tasks.records import TaskRequestRecord

    with pytest.raises(ValidationError):
        TaskRequestRecord.model_validate({"input": None, field: value})


@pytest.mark.parametrize("field", ["ttl_ms", "poll_interval_ms"])
def test_request_record_admits_full_u64_driver_contract(field):
    from veoveo_mcp.tasks.records import TaskRequestRecord

    record = TaskRequestRecord.model_validate({"input": None, field: 2**64 - 1})
    assert getattr(record, field) == 2**64 - 1


@pytest.mark.parametrize("field", ["ttl_ms", "poll_interval_ms"])
async def test_native_timing_metadata_preserves_full_u64_range(runtime, field):
    from decimal import Decimal

    async with asyncio.timeout(15):
        created = (await runtime.create(draft())).snapshot
        sql = test_query(f"test_task_storage/set_{field}.surql")
        await runtime.store.query(sql, {"task": task_record(created.task_id), "value": Decimal(2**64 - 1)})
        assert getattr(await runtime.get(str(created.task_id)), field) == 2**64 - 1
        with pytest.raises(StoreError):
            await runtime.store.query(sql, {"task": task_record(created.task_id), "value": Decimal(2**64)})
        assert getattr(await runtime.get(str(created.task_id)), field) == 2**64 - 1


async def expire_input_writer_lease(runtime, task_id):
    snapshot = await runtime.get(task_id)
    assert snapshot.status.value == "waiting"
    assert snapshot.lease_owner is not None
    assert snapshot.lease_expires_at is not None
    lease = await runtime.store.query(
        test_query("test_task_storage/input_writer_lease.surql"), {
            "task": task_record(snapshot.task_id),
            "worker": snapshot.lease_owner,
            "revision": snapshot._updated_at_exact.driver_value(),
        },
    )
    assert isinstance(lease[0], str), "input writer lease changed before fixture read"
    rows = await runtime.store.query(
        test_query("test_task_storage/expire_input_writer_lease.surql"), {
            "task": task_record(snapshot.task_id),
            "worker": snapshot.lease_owner,
            "expiry": lease[0],
        },
    )
    assert rows[0] is not None, "input writer lease changed before fixture handoff"


async def test_native_rust_python_task_storage_interop(runtime, surreal_platform):
    import json
    import os
    from pathlib import Path
    import subprocess
    from tempfile import TemporaryDirectory

    executable = os.environ.get("VEOVEO_TEST_TASK_RUNTIME_BIN")
    if not executable or not Path(executable).is_file() or not os.access(executable, os.X_OK):
        pytest.fail(
            "set VEOVEO_TEST_TASK_RUNTIME_BIN to the prebuilt surreal_integration "
            "test executable containing sdk_task_storage_interop; this fixture does not build it"
        )
    python_tasks = []
    python_failures = []
    for mode, tenant, payload in [
        ("direct", None, None),
        ("delegated", "local", [None, {"provider": {"unknown": True, "fraction": 1.5}}]),
        ("automated", "installation", {"large": 2**64 - 1, "null": None}),
    ]:
        caller = owner("storage-interop")
        authority = caller.authority.model_dump(mode="json")
        authority["tenant"] = tenant or "installation"
        authority["output_policy"]["data_labels"] = ["output"]
        authority["provenance"] = {"mode": mode}
        if mode != "automated":
            authority["provenance"]["initiator"] = caller.principal_key
        if mode == "delegated":
            authority["provenance"]["delegation_id"] = "delegation-interop"
        caller = replace(
            caller, tenant_key=tenant, data_labels=frozenset({"clearance"}),
            authority=type(caller.authority).model_validate(authority),
        )
        created = (await runtime.create(draft(
            owner=caller, request=payload, poll_interval_ms=2**64 - 1,
        ))).snapshot
        await runtime.claim(str(created.task_id), timedelta(seconds=60))
        await runtime.request_input(str(created.task_id), "interop", TaskInputRequest("owner/input", {"opaque": [None, 2**64 - 1]}))
        from veoveo_mcp.tasks.types import TaskFailure
        failure_value = {"code": "owner.extension", "message": "failure"}
        if mode != "direct":
            failure_value["details"] = None if mode == "delegated" else {"provider": [None, 2**64 - 1]}
        failure = TaskFailure.from_json(failure_value)
        failed = (await runtime.create(draft(owner=caller))).snapshot
        claimed_failure = (await runtime.claim(str(failed.task_id), timedelta(seconds=60))).snapshot
        await runtime.transition_if_current(claimed_failure, TaskTransition.failed(failure))
        python_failures.append({"task_id": str(failed.task_id), "owner": caller.to_json(), "failure": failure_value})
        python_tasks.append({
            "task_id": str(created.task_id), "owner": caller.to_json(), "request": payload,
            "poll_interval_ms": created.poll_interval_ms,
        })
    for written in python_tasks:
        await expire_input_writer_lease(runtime, written["task_id"])
    with TemporaryDirectory(prefix="veoveo-python-task-interop-") as temporary:
        directory = Path(temporary)
        config_path, output_path = directory / "config.json", directory / "output.json"
        config_path.write_text(json.dumps({
            **surreal_platform, "server": runtime.server, "python_tasks": python_tasks, "python_failures": python_failures,
        }), encoding="utf-8")
        config_path.chmod(0o600)
        environment = {
            "VEOVEO_TEST_TASK_INTEROP_CONFIG": str(config_path),
            "VEOVEO_TEST_TASK_INTEROP_OUTPUT": str(output_path),
        }
        if os.environ.get("LD_LIBRARY_PATH"):
            environment["LD_LIBRARY_PATH"] = os.environ["LD_LIBRARY_PATH"]
        try:
            result = await asyncio.to_thread(
                subprocess.run,
                [executable, "--ignored", "--exact", "sdk_task_storage_interop", "--nocapture"],
                env=environment, capture_output=True, text=True, timeout=120,
            )
        except subprocess.TimeoutExpired:
            pytest.fail("Rust Task storage interop exceeded its 120-second deadline")
        diagnostic = result.stderr + result.stdout
        for key in ("username", "password"):
            diagnostic = diagnostic.replace(surreal_platform[key], "<redacted>")
        assert result.returncode == 0, f"Rust Task storage interop failed: {diagnostic[-4096:]}"
        output = json.loads(output_path.read_text(encoding="utf-8"))
        assert set(output) == {"rust_tasks", "rust_failures"}
        assert len(output["rust_tasks"]) == len(python_tasks)
        assert {row["task_id"] for row in output["rust_tasks"]}.isdisjoint(
            row["task_id"] for row in python_tasks
        )
        for expected, written in zip(python_failures, output["rust_failures"], strict=True):
            assert written["failure"] == expected["failure"]
            assert (await runtime.get(written["task_id"])).error.to_json() == expected["failure"]
        for expected, written in zip(python_tasks, output["rust_tasks"], strict=True):
            assert set(written) == {"task_id", "owner", "request", "poll_interval_ms"}
            assert (OwnerContextRecord.model_validate(written["owner"]).to_owner()
                    == OwnerContextRecord.model_validate(expected["owner"]).to_owner())
            assert written["request"] == expected["request"]
            assert written["poll_interval_ms"] == expected["poll_interval_ms"]
            snapshot = await runtime.get(written["task_id"])
            assert snapshot.owner == OwnerContextRecord.model_validate(written["owner"]).to_owner()
            assert snapshot.request == written["request"]
            assert snapshot.poll_interval_ms == written["poll_interval_ms"]
            inputs = await runtime.outstanding_inputs(written["task_id"])
            assert inputs["interop"] == TaskInputRequest("owner/input", {"opaque": [None, 2**64 - 1]})
            from veoveo_mcp.tasks.runtime import _owner_context_record, _request_record
            comparisons = await runtime.store.query(
                test_query("test_task_storage/compare_claim_snapshot.surql"), {
                    "task": task_record(snapshot.task_id),
                    "expected_updated_at": snapshot._updated_at_exact.driver_value(),
                    "expected_request": _request_record(snapshot),
                    "expected_owner_context": _owner_context_record(snapshot.owner),
                },
            )
            assert all(comparisons[0][name] for name in (
                "timestamp_matches", "request_matches", "owner_matches",
            )), comparisons[0]
            await expire_input_writer_lease(runtime, written["task_id"])
            claimed = (await runtime.claim(written["task_id"], timedelta(seconds=30))).snapshot
            assert claimed.owner == snapshot.owner
            assert claimed.request == snapshot.request
            assert claimed.poll_interval_ms == snapshot.poll_interval_ms

        from surrealdb import Datetime
        import uuid

        delegated = python_tasks[1]
        caller = OwnerContextRecord.model_validate(delegated["owner"]).to_owner()
        extra = (await runtime.create(draft(owner=caller))).snapshot
        positions = [
            (delegated["task_id"], "2026-10-05T00:00:00.123456001Z"),
            (output["rust_tasks"][1]["task_id"], "2026-10-05T00:00:00.123456001Z"),
            (str(extra.task_id), "2026-10-05T00:00:00.123456002Z"),
        ]
        for expected, written in zip(python_failures, output["rust_failures"], strict=True):
            if OwnerContextRecord.model_validate(expected["owner"]).to_owner() == caller:
                positions.extend([
                    (expected["task_id"], "2026-10-05T00:00:00.123456003Z"),
                    (written["task_id"], "2026-10-05T00:00:00.123456004Z"),
                ])
        for task_id, timestamp in positions:
            await runtime.store.query(test_query("test_task_storage/set_created_timestamp.surql"), {
                "task": task_record(uuid.UUID(task_id)), "timestamp": Datetime(timestamp),
            })
        expected_ids = [task_id for task_id, timestamp in sorted(
            positions, key=lambda position: (position[1], uuid.UUID(position[0]).int),
        )]
        seen, cursor = [], None
        for _ in range(len(positions) + 1):
            page = await runtime.for_owner(caller).page(after=cursor, limit=1)
            seen.extend(str(item.task_id) for item in page.items)
            if page.next_cursor is None:
                break
            cursor = page.next_cursor
        else:
            pytest.fail("nanosecond Task pages did not terminate")
        assert seen == expected_ids
        assert len(seen) == len(set(seen))


@pytest.mark.parametrize("field,value", [
    ("context_key", "other-context"), ("membership", "owner"),
    ("policy_revision", "other-revision"), ("owner_kind", "principal"),
    ("owner_key", "other-owner"), ("classification", "classified"),
    ("data_labels", ["other-output"]), ("initial_grants", []),
    ("invocation_mode", "direct"), ("initiator_key", "different"),
    ("delegation_id", "other-delegation"),
])
def test_decoder_compares_every_flattened_authority_coordinate(stored_task, field, value):
    stored_task["authority"][field] = value
    with pytest.raises(InvalidRecord):
        _record_to_snapshot(stored_task)


def test_decoder_rejects_owner_authority_tenant_mismatch(stored_task):
    stored_task["owner_context"]["authority"]["tenant"] = "other-tenant"
    with pytest.raises(InvalidRecord):
        _record_to_snapshot(stored_task)


@pytest.mark.parametrize("field", ["ttl_ms", "poll_interval_ms"])
def test_decoder_and_expected_request_preserve_unsigned_metadata(stored_task, field):
    from decimal import Decimal
    from veoveo_mcp.tasks.runtime import _request_record

    stored_task["request"][field] = Decimal(2**64 - 1)
    snapshot = _record_to_snapshot(stored_task)
    assert getattr(snapshot, field) == 2**64 - 1
    assert _request_record(snapshot)[field] == Decimal(2**64 - 1)
    stored_task["request"][field] = Decimal(2**64)
    with pytest.raises(ValidationError):
        _record_to_snapshot(stored_task)


def test_driver_owner_and_request_defaults_are_explicit_and_labels_sorted(stored_task):
    from surrealdb.cbor import CBORSimpleValue
    from veoveo_mcp.tasks.runtime import _owner_context_record, _request_record

    caller = owner()
    authority = caller.authority.model_dump(mode="json")
    authority["tenant"] = "installation"
    authority["output_policy"]["data_labels"] = ["z", "a"]
    caller = replace(caller, tenant_key=None, data_labels=frozenset({"z", "a"}),
                     authority=type(caller.authority).model_validate(authority))
    encoded = _owner_context_record(caller)
    assert encoded["tenant_key"] == CBORSimpleValue(22)
    assert encoded["data_labels"] == ["a", "z"]
    assert encoded["authority"]["output_policy"]["data_labels"] == ["a", "z"]
    assert encoded["authority"]["output_policy"]["classification"] == CBORSimpleValue(22)
    stored_task["request"] = {"input": None}
    request = _request_record(_record_to_snapshot(stored_task))
    assert request == dict.fromkeys(
        ["input", "status_message", "ttl_ms", "poll_interval_ms"], CBORSimpleValue(22)
    )


@pytest.mark.parametrize("payload", [RecordID("task", "native"), datetime.now(timezone.utc)])
def test_request_decoder_rejects_record_and_datetime_values(stored_task, payload):
    from veoveo_mcp.tasks.records import TaskRequestRecord

    with pytest.raises(ValidationError):
        TaskRequestRecord.model_validate({"input": payload})
    stored_task["request"]["input"] = {"provider": [payload]}
    with pytest.raises(ValidationError):
        _record_to_snapshot(stored_task)


@pytest.mark.parametrize("payload", [
    None, True, 42, "scalar", [None, {"unknown": [1, "two"]}],
    {"provider": {"arbitrary": True}, "large": 2**64 - 1},
])
def test_request_decoder_preserves_every_json_shape(stored_task, payload):
    stored_task["request"]["input"] = payload
    assert _record_to_snapshot(stored_task).request == payload


@pytest.mark.parametrize("ttl", [10**15, 2**63, 2**64 - 1])
async def test_create_rejects_unrepresentable_ttl_before_database_access(ttl):
    from types import SimpleNamespace
    from unittest.mock import AsyncMock
    from veoveo_mcp.tasks import TaskRuntime

    store = SimpleNamespace(query=AsyncMock(), ensure_identity=AsyncMock())
    task_runtime = TaskRuntime(store, "datasheet", "worker")
    with pytest.raises(InvalidRecord, match="TTL exceeds supported deadline range"):
        await task_runtime.create(draft(ttl_ms=ttl, idempotency_key="existing"))
    store.query.assert_not_awaited()
    store.ensure_identity.assert_not_awaited()


@pytest.mark.parametrize("ttl,expected", [
    (None, timedelta(days=7)), (0, timedelta()), (1001, timedelta(milliseconds=1001)),
])
def test_ttl_none_zero_and_representable_expiration_keep_their_meaning(ttl, expected):
    from veoveo_mcp.tasks.types import default_retention_expiry

    now = datetime(2026, 10, 5, tzinfo=timezone.utc)
    assert default_retention_expiry(now, ttl) == now + expected


@pytest.mark.parametrize("field", ["ttl_ms", "poll_interval_ms"])
def test_decoder_normalizes_only_integral_timing_decimals(stored_task, field):
    from decimal import Decimal

    stored_task["request"][field] = Decimal("1.0")
    stored_task["request"]["input"] = {"fraction": Decimal("1.5")}
    snapshot = _record_to_snapshot(stored_task)
    assert getattr(snapshot, field) == 1
    assert snapshot.request == {"fraction": 1.5}
    stored_task["request"][field] = Decimal("1.5")
    with pytest.raises(ValidationError):
        _record_to_snapshot(stored_task)


@pytest.mark.parametrize("number", [float("nan"), float("inf"), float("-inf")])
@pytest.mark.parametrize("nested", [False, True])
async def test_request_rejects_nonfinite_json_before_database_effects(stored_task, number, nested):
    from types import SimpleNamespace
    from unittest.mock import AsyncMock
    from veoveo_mcp.tasks import TaskRuntime
    from veoveo_mcp.tasks.records import TaskRequestRecord

    payload = {"provider": [number]} if nested else number
    with pytest.raises(ValidationError):
        TaskRequestRecord.model_validate({"input": payload})
    stored_task["request"]["input"] = payload
    with pytest.raises(ValidationError):
        _record_to_snapshot(stored_task)
    store = SimpleNamespace(query=AsyncMock(), ensure_identity=AsyncMock())
    with pytest.raises(ValidationError):
        await TaskRuntime(store, "datasheet", "worker").create(draft(request=payload))
    store.query.assert_not_awaited()
    store.ensure_identity.assert_not_awaited()


@pytest.mark.parametrize("payload", [1.5, {"provider": [1.5, -0.25, 0.0]}])
def test_request_record_preserves_finite_json_fractions(stored_task, payload):
    from veoveo_mcp.tasks.records import TaskRequestRecord

    assert TaskRequestRecord.model_validate({"input": payload}).input == payload
    stored_task["request"]["input"] = payload
    assert _record_to_snapshot(stored_task).request == payload


@pytest.mark.parametrize("field,value", [
    ("profile", "Invalid Profile"), ("issuer", ""), ("data_labels", frozenset({""})),
])
async def test_create_validates_owner_before_database_effects(field, value):
    from types import SimpleNamespace
    from unittest.mock import AsyncMock
    from veoveo_mcp.tasks import TaskRuntime

    invalid = replace(owner(), **{field: value})
    store = SimpleNamespace(query=AsyncMock(), ensure_identity=AsyncMock())
    with pytest.raises(ValidationError):
        await TaskRuntime(store, "datasheet", "worker").create(
            draft(owner=invalid, idempotency_key="existing")
        )
    store.query.assert_not_awaited()
    store.ensure_identity.assert_not_awaited()


def test_snapshot_wire_preserves_exact_timestamp_without_extra_fields(stored_task):
    from veoveo_mcp.tasks import TaskSnapshot
    from veoveo_mcp.tasks.timestamp import TaskTimestamp

    text = "2026-10-05T00:00:00.123456789Z"
    stored_task["updated_at"] = TaskTimestamp(text).as_datetime()
    stored_task["updated_at_exact"] = text
    snapshot = _record_to_snapshot(stored_task)
    wire = snapshot.to_json()
    assert wire["updated_at"] == text
    assert "updated_at_exact" not in wire
    assert "_updated_at_exact" not in wire
    restored = TaskSnapshot.from_json(wire)
    assert restored._updated_at_exact == snapshot._updated_at_exact
    assert restored._updated_at_exact.driver_value().dt == text
    assert restored.to_json() == wire


@pytest.mark.parametrize("token", [None, "bad", "2026-10-05", "2026-10-05T00:00:00.1234567890Z"])
def test_decoder_requires_valid_exact_timestamp(stored_task, token):
    stored_task["updated_at_exact"] = token
    with pytest.raises((TypeError, ValueError)):
        _record_to_snapshot(stored_task)


def test_decoder_does_not_fall_back_when_exact_timestamp_is_missing(stored_task):
    del stored_task["updated_at_exact"]
    with pytest.raises(KeyError):
        _record_to_snapshot(stored_task)


async def test_native_exact_timestamp_is_derived_and_survives_history(runtime):
    from surrealdb import Datetime
    from veoveo_mcp.tasks.changefeed import changefeed_head

    async with asyncio.timeout(15):
        created = (await runtime.create(draft())).snapshot
        head = await changefeed_head(runtime.store)
        text = "2026-10-05T00:00:00.123456789Z"
        bindings = {"task": task_record(created.task_id), "timestamp": Datetime(text)}
        await runtime.store.query(test_query("test_task_storage/set_timestamp.surql"), bindings)
        snapshot = await runtime.get(str(created.task_id))
        assert snapshot.to_json()["updated_at"] == text
        try:
            await runtime.store.query(test_query("test_task_storage/forge_timestamp.surql"), bindings)
        except StoreError:
            pass
        assert (await runtime.get(str(created.task_id)))._updated_at_exact == snapshot._updated_at_exact
        history = await runtime.live_updates_after(head)
        try:
            async for update in history:
                if update.snapshot.task_id == created.task_id:
                    assert update.snapshot.to_json()["updated_at"] == text
                    break
        finally:
            await history.aclose()


async def test_native_same_microsecond_timestamp_change_rejects_stale_cas(runtime, monkeypatch):
    from surrealdb import Datetime
    from veoveo_mcp.tasks import TaskSnapshot

    async with asyncio.timeout(15):
        created = (await runtime.create(draft())).snapshot
        await runtime.claim(str(created.task_id), timedelta(seconds=30))
        first = "2026-10-05T00:00:00.123456001Z"
        second = "2026-10-05T00:00:00.123456002Z"
        bindings = {"task": task_record(created.task_id), "timestamp": Datetime(first)}
        sql = test_query("test_task_storage/set_timestamp.surql")
        await runtime.store.query(sql, bindings)
        snapshot = TaskSnapshot.from_json((await runtime.get(str(created.task_id))).to_json())
        original = runtime.store.query
        raced = False

        async def race(statement, parameters=None):
            nonlocal raced
            if "$expected_updated_at" in statement and not raced:
                raced = True
                await original(sql, {**bindings, "timestamp": Datetime(second)})
            return await original(statement, parameters)

        monkeypatch.setattr(runtime.store, "query", race)
        with pytest.raises(Conflict):
            await runtime.transition_if_current(snapshot, TaskTransition.succeeded("Done", {"ok": True}, result_uri=None))
        assert raced
        current = await runtime.get(str(created.task_id))
        assert current.updated_at == snapshot.updated_at
        assert current._updated_at_exact != snapshot._updated_at_exact
        assert current.to_json()["updated_at"] == second
        assert current.status == snapshot.status
        assert current.request == snapshot.request


def test_snapshot_construction_requires_exact_timestamp(stored_task):
    from dataclasses import fields
    from veoveo_mcp.tasks import TaskSnapshot

    snapshot = _record_to_snapshot(stored_task)
    values = {field.name: getattr(snapshot, field.name) for field in fields(snapshot)
              if field.name != "_updated_at_exact"}
    with pytest.raises(TypeError, match="_updated_at_exact"):
        TaskSnapshot(**values)


def test_snapshot_rejects_timestamp_view_disagreement(stored_task):
    stored_task["updated_at_exact"] = "2026-10-05T00:00:00.123456789Z"
    with pytest.raises(InvalidRecord, match="datetime view"):
        _record_to_snapshot(stored_task)


@pytest.mark.parametrize("left,right", [
    ("2026-10-05T00:00:00.123456789Z", "2026-10-05T02:00:00.123456789+02:00"),
    ("2026-10-05T00:00:00.1Z", "2026-10-05T00:00:00.100000000+00:00"),
    ("2026-10-05T00:00:00Z", "2026-10-04T23:00:00.000000000-01:00"),
])
def test_exact_timestamps_compare_instants_without_losing_wire_spelling(left, right):
    from veoveo_mcp.tasks import TaskTimestamp

    first, second = TaskTimestamp(left), TaskTimestamp(right)
    assert first.same_instant(second)
    assert str(first) == left
    assert str(second) == right
    assert not TaskTimestamp("2026-10-05T00:00:00.123456001Z").same_instant(
        TaskTimestamp("2026-10-05T00:00:00.123456002Z")
    )


def test_creation_timestamp_wire_and_page_cursor_are_lossless(stored_task):
    from veoveo_mcp.tasks import TaskPageCursor, TaskSnapshot, TaskTimestamp

    text = "2026-10-05T00:00:00.123456789Z"
    stored_task["created_at"] = TaskTimestamp(text).as_datetime()
    stored_task["created_at_exact"] = text
    snapshot = _record_to_snapshot(stored_task)
    assert snapshot.to_json()["created_at"] == text
    assert TaskSnapshot.from_json(snapshot.to_json())._created_at_exact == TaskTimestamp(text)
    cursor = TaskPageCursor(snapshot._created_at_exact, snapshot.task_id)
    assert cursor.created_at.driver_value().dt == text
    with pytest.raises(TaskError, match="lossless typed timestamp"):
        TaskPageCursor(snapshot.created_at, snapshot.task_id)


def test_mcp_completion_admits_product_plain_and_addressable_tool_error():
    from veoveo_mcp.tasks import mcp_task_completion
    from veoveo_mcp.types import ResourceUri

    uri = ResourceUri("fixture://items/1")
    product = {
        "content": [{"type": "text", "text": "done"},
                    {"type": "resource_link", "uri": str(uri), "name": "item"}],
        "structuredContent": {"result_uri": str(uri)}, "isError": False,
    }
    assert mcp_task_completion("done", product).result_uri() == uri
    assert mcp_task_completion("done", {**product, "isError": True}).result_uri() == uri
    for error in [False, True]:
        assert mcp_task_completion("done", {"content": [], "isError": error}).result_uri() is None
    for malformed in [None, 42, "not a resource address", "fixture://items/2"]:
        with pytest.raises(InvalidRecord):
            mcp_task_completion("done", {**product, "structuredContent": {"result_uri": malformed}})
    with pytest.raises(InvalidRecord):
        mcp_task_completion("done", {**product, "content": []})
    with pytest.raises(InvalidRecord):
        mcp_task_completion("done", {**product, "content": product["content"] + [product["content"][1]]})


def test_mcp_completion_preserves_open_nulls_and_rejects_explicit_null_addresses():
    from mcp.types import CallToolResult
    from veoveo_mcp.tasks import mcp_task_completion

    for typed in [False, True]:
        payload = {"content": [], "structuredContent": {"result_uri": None}, "isError": True}
        result = CallToolResult.model_validate(payload) if typed else payload
        with pytest.raises(InvalidRecord):
            mcp_task_completion("done", result)
        payload = {"content": [], "structuredContent": {"provider_field": None, "nested": {"null": None}}}
        result = CallToolResult.model_validate(payload) if typed else payload
        transition = mcp_task_completion("done", result)
        assert transition.result_uri() is None
        assert transition.result().payload["structuredContent"] == payload["structuredContent"]
    with pytest.raises(InvalidRecord):
        mcp_task_completion("done", {"content": [{"type": "resource_link", "name": "item", "uri": "fixture://items/1"}]})


def test_task_completion_requires_explicit_typed_product_address(stored_task):
    from veoveo_mcp.tasks import TaskSnapshot, TaskStatus
    from veoveo_mcp.tasks.store import task_result_to_store
    from veoveo_mcp.types import ResourceUri

    with pytest.raises(TypeError):
        TaskTransition.succeeded("done", {})
    with pytest.raises(InvalidRecord):
        TaskTransition.succeeded("done", {}, result_uri="fixture://items/1")
    uri = ResourceUri("fixture://items/1")
    with pytest.raises(InvalidRecord):
        TaskTransition(TaskStatus.FAILED, "failed", result_uri=uri)
    stored_task.update(status="succeeded", result=task_result_to_store(TaskTransition.succeeded("done", {"ok": True}, result_uri=uri).result()), result_uri=str(uri))
    snapshot = _record_to_snapshot(stored_task)
    assert snapshot.result_uri == uri
    assert TaskSnapshot.from_json(snapshot.to_json()).result_uri == uri
    assert _record_to_snapshot({**stored_task, "result_uri": None}).result_uri is None
    for mutation in [{"status": "queued"}, {"result": None}, {"result_uri": "invalid URI"}, {"result_uri": 42}]:
        with pytest.raises((InvalidRecord, TypeError, ValueError)):
            _record_to_snapshot({**stored_task, **mutation})


@pytest.mark.parametrize("selection", ["trusted", "owner", "context", "types", "context_types"])
@pytest.mark.parametrize("is_error", [False, True])
async def test_product_completion_persists_address_with_result_in_every_transition_profile(runtime, selection, is_error):
    from veoveo_mcp.tasks import TaskSnapshot, TaskTypeName, mcp_task_completion
    from veoveo_mcp.types import ResourceUri

    async with asyncio.timeout(15):
        request = draft()
        created = (await runtime.create(request)).snapshot
        claimed = (await runtime.claim(str(created.task_id), timedelta(seconds=30))).snapshot
        query = None if selection == "trusted" else runtime.for_owner(request.owner)
        if selection in ("context", "context_types"):
            query = query.in_work_context()
        if selection in ("types", "context_types"):
            query = query.of_type(TaskTypeName(request.task_type))
        uri = ResourceUri("fixture://items/1")
        payload = {"content": [{"type": "resource_link", "uri": str(uri), "name": "item"}],
                   "structuredContent": {"result_uri": str(uri), "metadata": {"null": None}}, "isError": is_error}
        completed = await runtime.transition_if_current(claimed, mcp_task_completion("done", payload), owner_query=query)
        retained = await runtime.get(str(created.task_id)) if query is None else await query.get(created.task_id)
        assert completed.result_uri == retained.result_uri == uri
        assert retained.result.payload["isError"] == is_error
        assert retained.result.payload["structuredContent"] == payload["structuredContent"]
        replay = TaskSnapshot.from_json(retained.to_json())
        assert replay.result_uri == uri and replay.result == retained.result


@pytest.mark.parametrize("value", [
    {"code": "owner.extension", "message": "failure"},
    {"code": "owner.extension", "message": "failure", "details": None},
    {"code": "owner.extension", "message": "failure", "details": {"provider": [None, 2**64 - 1]}},
])
def test_failure_envelope_roundtrip_preserves_absent_null_and_provider_details(value):
    from veoveo_mcp.tasks.types import TaskFailure
    from veoveo_mcp.tasks.records import TaskFailureRecord
    from veoveo_mcp.tasks.store import _json_to_surreal
    record = TaskFailureRecord.model_validate(value)
    stored = _json_to_surreal(record.model_dump(mode="json", exclude_unset=True))
    # Native CBOR null is decoded by the real SDK; emulate its decoded value here.
    assert TaskFailure.from_json(value).to_json() == value
    assert set(stored) == set(value)


@pytest.mark.parametrize("method", ["", "\n", "\u0085", "é" * 129])
def test_retained_input_request_applies_method_admission(method):
    from veoveo_mcp.tasks.records import TaskInputRequestRecord
    with pytest.raises((ValidationError, InvalidRecord)):
        TaskInputRequestRecord.model_validate({"method": method, "params": {}})


@pytest.mark.parametrize("value", [
    {"code": "x"}, {"code": 42, "message": "x"},
    {"code": "x", "message": "x", "unexpected": True},
])
def test_failure_decoder_rejects_uncontrolled_shape(value):
    from veoveo_mcp.tasks.types import TaskFailure
    with pytest.raises((ValidationError, InvalidRecord)):
        TaskFailure.from_json(value)


def _json_after_driver_cbor(value):
    # The outbound NULL marker becomes ordinary None only after CBOR decoding.
    from surrealdb.data.cbor import decode, encode
    from veoveo_mcp.tasks.store import _json_from_surreal
    return _json_from_surreal(decode(encode(value)))


def test_native_create_and_idempotency_records_keep_references_datetimes_and_sorted_sets():
    from decimal import Decimal
    from veoveo_mcp.tasks.records import TaskCreateRecord, TaskIdempotencyRecord

    caller = owner()
    authority = caller.authority.model_dump(mode="json")
    authority["output_policy"]["data_labels"] = ["zulu", "alpha"]
    from veoveo_mcp.contract import InvocationAuthority
    caller = replace(caller, authority=InvocationAuthority.model_validate(authority),
                     data_labels=frozenset({"zulu", "alpha"}))
    request = draft(owner=caller, request={"provider": [None, 2**64 - 1, 1.5]},
                    poll_interval_ms=2**64 - 1, idempotency_key="same-operation",
                    retention_pins=frozenset({"zulu", "alpha"}))
    now = datetime.now(timezone.utc)
    expiry = now + timedelta(days=7)
    content = TaskCreateRecord.from_draft(
        request, work_context=_work_context_record(caller), initiator=_initiator_record(caller),
        now=now, retention_expires_at=expiry,
    ).to_native()
    for field in ("tenant", "owner", "work_context", "profile", "server"):
        assert isinstance(content[field], RecordID)
    assert content["created_at"] is now
    assert content["updated_at"] is now
    assert content["retention_expires_at"] is expiry
    assert content["result"] is None  # Native NONE, not a present JSON-null result.
    assert content["request"]["poll_interval_ms"] == Decimal(2**64 - 1)
    assert _json_after_driver_cbor(content["request"])["input"] == request.request
    assert content["owner_context"]["data_labels"] == ["alpha", "zulu"]
    assert content["owner_context"]["authority"]["output_policy"]["data_labels"] == ["alpha", "zulu"]
    assert content["authority"]["data_labels"] == ["alpha", "zulu"]
    assert content["retention_pins"] == ["alpha", "zulu"]
    assert content["status"] == "queued"
    assert content["recovery_class"] == request.recovery_class.value
    link = TaskIdempotencyRecord.from_draft(request, task_record(request.task_id), now).to_native()
    assert link["task"] == task_record(request.task_id)
    assert link["tenant"] == content["tenant"]
    assert link["owner"] == content["owner"]
    assert link["server"] == content["server"]
    assert link["created_at"] is now


def test_native_input_and_failure_records_preserve_json_null_and_absent_details(stored_task):
    from veoveo_mcp.tasks import TaskFailure
    from veoveo_mcp.tasks.records import TaskFailureRecord, TaskInputRecord, TaskRequestRecord
    from veoveo_mcp.tasks.runtime import _request_record

    stored_task["request"]["poll_interval_ms"] = None
    snapshot = _record_to_snapshot(stored_task)
    now = datetime.now(timezone.utc)
    request = TaskInputRequest("elicitation/create", {"provider": [None, 2**64 - 1]})
    content = TaskInputRecord.from_request(task_record(snapshot.task_id), "answer", request, now).to_native()
    assert isinstance(content["task"], RecordID)
    assert content["created_at"] is now
    assert content["response"] is None
    assert content["responded_at"] is None
    assert _json_after_driver_cbor(content["request"])["params"] == request.params
    assert TaskRequestRecord.from_snapshot(snapshot, status_message=snapshot.status_message).to_native() == _request_record(snapshot)
    decoded = _json_after_driver_cbor(_request_record(snapshot))
    assert "poll_interval_ms" in decoded and decoded["poll_interval_ms"] is None
    for value in [{"code": "provider", "message": "failed"},
                  {"code": "provider", "message": "failed", "details": None},
                  {"code": "provider", "message": "failed", "details": {"opaque": [None, 2**64 - 1]}}]:
        assert _json_after_driver_cbor(TaskFailureRecord.from_failure(TaskFailure.from_json(value)).to_native()) == value


@pytest.mark.parametrize("payload", [RecordID("task", "native"), datetime.now(timezone.utc)])
def test_native_writers_keep_controlled_native_values_out_of_open_json(payload):
    from veoveo_mcp.tasks.records import TaskCreateRecord, TaskInputRecord
    request = draft(request={"provider": [payload]})
    now = datetime.now(timezone.utc)
    with pytest.raises(ValidationError):
        TaskCreateRecord.from_draft(request, work_context=_work_context_record(request.owner),
                                   initiator=_initiator_record(request.owner), now=now,
                                   retention_expires_at=now + timedelta(days=7))
    with pytest.raises(ValidationError):
        TaskInputRecord.from_request(task_record(request.task_id), "answer",
                                     TaskInputRequest("elicitation/create", {"provider": payload}), now)
