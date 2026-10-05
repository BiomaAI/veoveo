"""Durable execution state for Veoveo MCP tasks, ported from `veoveo-task-runtime`.

SurrealDB is the sole task authority. Transactions commit execution state;
native changefeeds supply ordered history and LIVE queries wake readers.
"""

from __future__ import annotations

import asyncio
import uuid
from datetime import datetime, timedelta, timezone
from typing import Any, AsyncIterator

from surrealdb import RecordID

from .queries import OwnerStatement, query
from .owner_query import OwnerTaskQuery

from .store import (
    MAX_TRANSACTION_ATTEMPTS,
    StoreError,
    SurrealStore,
    task_result_from_store,
    task_result_to_store,
)
from .types import (
    ClaimedTask,
    Conflict,
    CreateTask,
    allowed_transition,
    CreateTaskResult,
    DuplicateInputKey,
    InvalidProgress,
    InvalidRecord,
    InvalidTransition,
    LeaseHeld,
    RecoveryClass,
    RecoveryReport,
    TaskError,
    TaskFailure,
    TaskInputExchange,
    TaskInputRequest,
    TaskInputSubmission,
    TaskNotFound,
    TaskOwner,
    TaskSnapshot,
    TaskStatus,
    TaskTransition,
    TaskUpdate,
    TaskUpdateCursor,
    WrongServer,
    default_retention_expiry,
    deterministic_principal_id,
    deterministic_tenant_id,
    deterministic_work_context_id,
    idempotency_record,
    parse_task_id,
    profile_record,
    server_record,
    task_input_record,
    task_record,
    validate_input_key,
    validate_input_method,
)


def _now() -> datetime:
    return datetime.now(timezone.utc)


def _work_context_record(owner: TaskOwner) -> RecordID:
    return RecordID(
        "work_context",
        deterministic_work_context_id(
            owner.effective_tenant_key(), owner.authority.work_context
        ),
    )


def _initiator_record(owner: TaskOwner) -> RecordID | None:
    initiator = owner.authority.initiator
    if initiator is None:
        return None
    return RecordID(
        "principal",
        deterministic_principal_id(owner.effective_tenant_key(), initiator),
    )


def _authority_record(owner: TaskOwner) -> dict[str, Any]:
    authority = owner.authority
    output = authority.output_policy
    return {
        "context_key": authority.work_context,
        "membership": authority.membership.value,
        "policy_revision": authority.policy_revision,
        "owner_kind": output.owner.kind,
        "owner_key": output.owner.id,
        "initial_grants": [
            {
                "subject_kind": grant.subject.kind,
                "subject_key": grant.subject.id,
                "permission": grant.level.value,
            }
            for grant in output.initial_grants
        ],
        "classification": output.classification,
        "data_labels": sorted(output.data_labels),
        "invocation_mode": authority.invocation_mode,
        "initiator_key": authority.initiator,
        "delegation_id": authority.delegation_id,
    }


class TaskRuntime:
    def __init__(self, store: SurrealStore, server: str, worker_id: str) -> None:
        self.store = store
        self.server = server
        self.worker_id = worker_id
        self._workers: dict[uuid.UUID, tuple[asyncio.Event, asyncio.Task]] = {}

    @classmethod
    async def connect(
        cls,
        endpoint: str,
        namespace: str,
        database: str,
        username: str,
        password: str,
        server: str,
        worker_id: str,
    ) -> "TaskRuntime":
        store = await SurrealStore.connect(
            endpoint, namespace, database, username, password
        )
        return cls(store, server, worker_id)

    def platform_store(self) -> SurrealStore:
        return self.store

    async def create(self, draft: CreateTask) -> CreateTaskResult:
        if draft.server != self.server:
            raise WrongServer(draft.server)
        if draft.idempotency_key is not None:
            existing = await self._idempotent_task(draft.owner, draft.idempotency_key)
            if existing is not None:
                return CreateTaskResult(snapshot=existing, created=False)

        await self.store.ensure_identity(draft.owner)

        record = task_record(draft.task_id)
        now = _now()
        retention = default_retention_expiry(now, draft.ttl_ms)
        envelope = {
            "input": draft.request,
            "owner": draft.owner.to_json(),
            "status_message": "Queued",
            "ttl_ms": draft.ttl_ms,
            "poll_interval_ms": draft.poll_interval_ms,
        }
        content = {
            "tenant": draft.owner.tenant_record(),
            "owner": draft.owner.principal_record(),
            "work_context": _work_context_record(draft.owner),
            "initiator": _initiator_record(draft.owner),
            "invocation_mode": draft.owner.authority.invocation_mode,
            "delegation_id": draft.owner.authority.delegation_id,
            "policy_revision": draft.owner.authority.policy_revision,
            "authority": _authority_record(draft.owner),
            "profile": profile_record(draft.owner.profile),
            "server": server_record(self.server),
            "task_type": draft.task_type,
            "status": TaskStatus.QUEUED.value,
            "recovery_class": draft.recovery_class.value,
            "request": envelope,
            "progress": 0.0,
            "result": None,
            "error": None,
            "result_artifact": None,
            "idempotency_key": draft.idempotency_key,
            "lease_owner": None,
            "lease_expires_at": None,
            "cancel_requested_at": None,
            "created_at": now,
            "updated_at": now,
            "started_at": None,
            "completed_at": None,
            "retention_expires_at": retention,
            "retention_pins": sorted(draft.retention_pins),
            "search_text": f"{self.server} {draft.task_type} {draft.owner.principal_key}",
        }
        if draft.idempotency_key is not None:
            idempotency = idempotency_record(
                draft.owner, self.server, draft.idempotency_key
            )
            link = {
                "task": record,
                "tenant": draft.owner.tenant_record(),
                "owner": draft.owner.principal_record(),
                "server": server_record(self.server),
                "key": draft.idempotency_key,
                "created_at": now,
            }
            for attempt in range(MAX_TRANSACTION_ATTEMPTS):
                try:
                    await self.store.query(
                        query("runtime/create_2.surql"),
                        {
                            "idempotency": idempotency,
                            "link": link,
                            "task": record,
                            "content": content,
                        },
                    )
                    break
                except StoreError as error:
                    existing = await self._idempotent_task(
                        draft.owner, draft.idempotency_key
                    )
                    if existing is not None:
                        return CreateTaskResult(snapshot=existing, created=False)
                    if error.retryable and attempt + 1 < MAX_TRANSACTION_ATTEMPTS:
                        await asyncio.sleep((1 << attempt) / 1_000)
                        continue
                    raise
        else:
            await self.store.query(
                query("runtime/create.surql"),
                {"task": record, "content": content},
            )

        snapshot = await self.get(str(draft.task_id))
        if snapshot is None:
            raise TaskNotFound(str(draft.task_id))

        return CreateTaskResult(snapshot=snapshot, created=True)

    async def get(self, task_id: str) -> TaskSnapshot | None:
        parsed = parse_task_id(task_id)
        rows = await self.store.query(
            query("runtime/get.surql"),
            {"task": task_record(parsed), "server": server_record(self.server)},
        )
        records = rows[0] or []
        return _record_to_snapshot(records[0]) if records else None

    async def list(self) -> list[TaskSnapshot]:
        rows = await self.store.query(
            query("runtime/list.surql"),
            {"server": server_record(self.server)},
        )
        return [_record_to_snapshot(record) for record in rows[0] or []]

    def for_owner(self, owner: TaskOwner) -> OwnerTaskQuery:
        return OwnerTaskQuery(self, owner)

    async def owner(self, task_id: str) -> TaskOwner | None:
        snapshot = await self.get(task_id)
        return snapshot.owner if snapshot is not None else None

    async def acknowledge_retention_pin(self, task_id: str, pin: str) -> TaskSnapshot:
        parsed = parse_task_id(task_id)
        rows = await self.store.query(
            query("runtime/acknowledge_retention_pin.surql"),
            {
                "task": task_record(parsed),
                "pin": pin,
                "server": server_record(self.server),
            },
        )
        if rows[0] is not None:
            return _record_to_snapshot(rows[0])
        snapshot = await self.get(str(parsed))
        if snapshot is None:
            raise TaskNotFound(str(parsed))
        return snapshot

    async def request_input(
        self, task_id: str, key: str, request: TaskInputRequest
    ) -> TaskInputExchange:
        validate_input_key(key)
        validate_input_method(request.method)
        current = await self.get(task_id)
        if current is None:
            raise TaskNotFound(task_id)
        if current.status not in (
            TaskStatus.QUEUED,
            TaskStatus.RUNNING,
            TaskStatus.WAITING,
        ):
            raise InvalidTransition(current.status, TaskStatus.WAITING)
        now = _now()
        if current.lease_owner != self.worker_id or (
            current.lease_expires_at is None or current.lease_expires_at <= now
        ):
            raise LeaseHeld(task_id)

        input_id = task_input_record(current.task_id, key)
        content = {
            "task": task_record(current.task_id),
            "request_key": key,
            "request": {"method": request.method, "params": request.params},
            "response": None,
            "created_at": now,
            "responded_at": None,
        }
        envelope = {
            "input": current.request,
            "owner": current.owner.to_json(),
            "status_message": "Waiting for input",
            "ttl_ms": current.ttl_ms,
            "poll_interval_ms": current.poll_interval_ms,
        }
        try:
            await self.store.query(
                query("runtime/request_input.surql"),
                {
                    "task": task_record(current.task_id),
                    "request": envelope,
                    "now": now,
                    "expected_updated_at": current.updated_at,
                    "server": server_record(self.server),
                    "tenant": current.owner.tenant_record(),
                    "owner": current.owner.principal_record(),
                    "worker": self.worker_id,
                    "input": input_id,
                    "content": content,
                },
            )
        except StoreError as error:
            if await self._input_exchange_by_id(input_id) is not None:
                raise DuplicateInputKey(key) from error
            recheck = await self.get(task_id)
            if recheck is None or recheck.updated_at != current.updated_at:
                raise Conflict(task_id) from error
            raise

        exchange = await self._input_exchange_by_id(input_id)
        if exchange is None:
            raise InvalidRecord("task input readback is missing")
        return exchange

    async def outstanding_inputs(self, task_id: str | uuid.UUID) -> dict[str, TaskInputRequest]:
        parsed = parse_task_id(task_id)
        if await self.get(str(parsed)) is None:
            raise TaskNotFound(str(parsed))
        rows = await self.store.query(
            query("runtime/outstanding_inputs.surql"),
            {"task": task_record(parsed)},
        )
        outstanding: dict[str, TaskInputRequest] = {}
        for record in rows[0] or []:
            exchange = _input_record_to_exchange(record)
            outstanding[exchange.key] = exchange.request
        return outstanding

    async def submit_input_responses(
        self, task_id: str, responses: dict[str, dict[str, Any]],
        *, owner_query: OwnerTaskQuery | None = None,
    ) -> TaskInputSubmission:
        current = (
            await self.get(task_id) if owner_query is None
            else await owner_query.get(parse_task_id(task_id))
        )
        if current is None:
            raise TaskNotFound(task_id)
        if current.is_terminal() or current.status == TaskStatus.CANCEL_REQUESTED:
            raise InvalidTransition(current.status, TaskStatus.RUNNING)
        submission = TaskInputSubmission()
        for key, response_value in responses.items():
            validate_input_key(key)
            attempt = 0
            while True:
                now = _now()
                try:
                    results = await self.store.query(
                        (
                            query("runtime/submit_input_responses.surql")
                            if owner_query is None
                            else owner_query._statement(OwnerStatement.INPUT_RESPONSES)
                        ),
                        {
                            "input": task_input_record(current.task_id, key),
                            "response": response_value,
                            "now": now,
                            "task": task_record(current.task_id),
                            "server": server_record(self.server),
                            **(owner_query.bindings() if owner_query is not None else {}),
                        },
                    )
                    accepted = results[3]
                    break
                except StoreError as error:
                    if error.retryable and attempt + 1 < MAX_TRANSACTION_ATTEMPTS:
                        await asyncio.sleep((1 << attempt) / 1_000)
                        attempt += 1
                        continue
                    raise
            if accepted is not None:
                submission.accepted += 1
            else:
                submission.ignored += 1
        return submission

    async def live_updates(self) -> AsyncIterator[TaskUpdate]:
        from .history import history_updates

        return await history_updates(self)

    async def live_updates_after(
        self, cursor: TaskUpdateCursor
    ) -> AsyncIterator[TaskUpdate]:
        """Resume the complete final transaction; consumers accept duplicate states."""
        from .history import history_updates

        return await history_updates(self, cursor)

    async def claim(self, task_id: str, lease_duration: timedelta) -> ClaimedTask:
        if lease_duration <= timedelta(0):
            raise InvalidRecord("task lease duration must be greater than zero")
        snapshot = await self.get(task_id)
        if snapshot is None:
            raise TaskNotFound(task_id)
        if snapshot.server != self.server:
            raise WrongServer(task_id)
        now = _now()
        if (
            snapshot.lease_expires_at is not None
            and snapshot.lease_expires_at > now
            and snapshot.lease_owner != self.worker_id
        ):
            raise LeaseHeld(task_id)
        if snapshot.is_terminal() or snapshot.status == TaskStatus.CANCEL_REQUESTED:
            raise InvalidTransition(snapshot.status, TaskStatus.RUNNING)
        lease_expires_at = now + lease_duration
        envelope = {
            "input": snapshot.request,
            "owner": snapshot.owner.to_json(),
            "status_message": "Running",
            "ttl_ms": snapshot.ttl_ms,
            "poll_interval_ms": snapshot.poll_interval_ms,
        }
        results = await self.store.query(
            query("runtime/claim.surql"),
            {
                "task": task_record(snapshot.task_id),
                "worker": self.worker_id,
                "request": envelope,
                "lease_expires": lease_expires_at,
                "now": now,
                "expected": snapshot.status.value,
                "expected_updated_at": snapshot.updated_at,
            },
        )
        updated = results[2]
        if updated is None:
            raise Conflict(task_id)

        return ClaimedTask(
            snapshot=_record_to_snapshot(updated),
            lease_owner=self.worker_id,
            lease_expires_at=lease_expires_at,
        )

    async def renew_lease(self, task_id: str, lease_duration: timedelta) -> TaskSnapshot:
        if lease_duration <= timedelta(0):
            raise InvalidRecord("task lease duration must be greater than zero")
        parsed = parse_task_id(task_id)
        if await self.get(str(parsed)) is None:
            raise TaskNotFound(str(parsed))
        now = _now()
        rows = await self.store.query(
            query("runtime/renew_lease.surql"),
            {
                "task": task_record(parsed),
                "worker": self.worker_id,
                "lease_expires": now + lease_duration,
                "now": now,
            },
        )
        if rows[0] is None:
            raise LeaseHeld(str(parsed))
        return _record_to_snapshot(rows[0])

    async def transition(
        self, task_id: str, transition: TaskTransition
    ) -> TaskSnapshot:
        current = await self.get(task_id)
        if current is None:
            raise TaskNotFound(task_id)
        return await self.transition_if_current(current, transition)

    async def transition_if_current(
        self, current: TaskSnapshot, transition: TaskTransition,
        *, owner_query: OwnerTaskQuery | None = None,
    ) -> TaskSnapshot:
        task_id = str(current.task_id)
        if current.server != self.server:
            raise WrongServer(task_id)
        durable = (
            await self.get(task_id) if owner_query is None
            else await owner_query.get(current.task_id)
        )
        if durable is None:
            raise TaskNotFound(task_id)
        if durable.status != current.status or durable.updated_at != current.updated_at:
            raise Conflict(task_id)
        next_status = transition.status()
        if not allowed_transition(current.status, next_status):
            raise InvalidTransition(current.status, next_status)
        progress = transition.progress(current.progress)
        if not (progress == progress and 0.0 <= progress <= 1.0):  # NaN-safe
            raise InvalidProgress()
        now = _now()
        control_transition = next_status == TaskStatus.CANCEL_REQUESTED
        expired_cancellation = (
            current.status == TaskStatus.CANCEL_REQUESTED
            and next_status == TaskStatus.CANCELLED
        )
        if (
            not control_transition
            and not expired_cancellation
            and durable.lease_owner != self.worker_id
        ):
            raise LeaseHeld(task_id)
        terminal = next_status.is_terminal()
        message = transition.message()
        envelope = {
            "input": current.request,
            "owner": current.owner.to_json(),
            "status_message": message,
            "ttl_ms": current.ttl_ms,
            "poll_interval_ms": current.poll_interval_ms,
        }
        result = transition.result()
        failure = transition.failure()
        results = await self.store.query(
            (
                query("runtime/transition_if_current.surql")
                if owner_query is None
                else owner_query._statement(OwnerStatement.TRANSITION)
            ),
            {
                "task": task_record(current.task_id),
                "next": next_status.value,
                "request": envelope,
                "progress": progress,
                "result": task_result_to_store(result),
                "error": failure.to_json() if failure is not None else None,
                "cancel_requested_at": (
                    now
                    if next_status == TaskStatus.CANCEL_REQUESTED
                    else current.cancel_requested_at
                ),
                "completed_at": now if terminal else None,
                "terminal": terminal,
                "now": now,
                "expected": current.status.value,
                "expected_updated_at": current.updated_at,
                "server": server_record(self.server),
                "tenant": current.owner.tenant_record(),
                "owner": current.owner.principal_record(),
                "worker": self.worker_id,
                "control_transition": control_transition,
                "expired_cancellation": expired_cancellation,
                **(owner_query.bindings() if owner_query is not None else {}),
            },
        )
        updated = results[2]
        if updated is None:
            raise Conflict(task_id)

        return _record_to_snapshot(updated)

    async def cancel(
        self, task_id: str, *, owner_query: OwnerTaskQuery | None = None
    ) -> TaskSnapshot:
        while True:
            current = (
                await self.get(task_id) if owner_query is None
                else await owner_query.get(parse_task_id(task_id))
            )
            if current is None:
                raise TaskNotFound(task_id)
            if current.is_terminal():
                return current
            if current.status == TaskStatus.CANCEL_REQUESTED:
                requested = current
            else:
                try:
                    requested = await self.transition_if_current(
                        current, TaskTransition.cancel_requested(), owner_query=owner_query
                    )
                except Conflict:
                    continue
            worker = self._workers.get(requested.task_id)
            if worker is not None:
                worker[0].set()
            if requested.lease_owner is None:
                try:
                    return await self.transition_if_current(
                        requested, TaskTransition.cancelled(), owner_query=owner_query
                    )
                except Conflict:
                    continue
            return requested

    async def is_cancel_requested(self, task_id: str) -> bool:
        snapshot = await self.get(task_id)
        return snapshot is not None and snapshot.status == TaskStatus.CANCEL_REQUESTED

    async def await_terminal(self, task_id: str) -> TaskSnapshot:
        """Read current state on native Task changes, including other replicas."""
        wake = await self.store.task_wake()
        try:
            while True:
                snapshot = await self.get(task_id)
                if snapshot is None:
                    raise TaskNotFound(task_id)
                if snapshot.is_terminal():
                    return snapshot
                await wake.wait()
        finally:
            await wake.close()

    def register_worker(
        self, task_id: str, cancellation: asyncio.Event, task: asyncio.Task
    ) -> None:
        self._workers[parse_task_id(task_id)] = (cancellation, task)

    def reap_workers(self) -> None:
        self._workers = {
            task_id: worker
            for task_id, worker in self._workers.items()
            if not worker[1].done()
        }

    async def recover(self) -> RecoveryReport:
        report = RecoveryReport()
        for task in await self.list():
            if task.lease_expires_at is not None and task.lease_expires_at > _now():
                continue
            if task.status == TaskStatus.QUEUED:
                if task.recovery_class == RecoveryClass.WEBHOOK_WAIT:
                    waiting = await self._recovery_result(self._force_waiting(task))
                    if waiting is not None:
                        report.webhook_waiting.append(waiting)
                else:
                    report.resumable.append(task)
            elif task.status == TaskStatus.CANCEL_REQUESTED:
                cancelled = await self._recovery_result(
                    self.transition_if_current(task, TaskTransition.cancelled())
                )
                if cancelled is not None:
                    report.cancelled.append(cancelled)
            elif task.status in (TaskStatus.RUNNING, TaskStatus.WAITING):
                if task.recovery_class == RecoveryClass.RESUME:
                    reset = await self._recovery_result(self._reset_for_recovery(task))
                    if reset is not None:
                        report.resumable.append(reset)
                elif task.recovery_class == RecoveryClass.WEBHOOK_WAIT:
                    if task.status == TaskStatus.WAITING:
                        report.webhook_waiting.append(task)
                    else:
                        waiting = await self._recovery_result(self._force_waiting(task))
                        if waiting is not None:
                            report.webhook_waiting.append(waiting)
                else:
                    failed = await self._recovery_result(
                        self._force_failed(task, TaskFailure.interrupted_indeterminate())
                    )
                    if failed is not None:
                        report.failed_indeterminate.append(failed)
        return report

    async def prune_expired(self) -> list[uuid.UUID]:
        results = await self.store.query(
            query("runtime/prune_expired.surql"),
            {"now": _now()},
        )
        return [_record_to_snapshot(record).task_id for record in results[0] or []]

    async def _idempotent_task(self, owner: TaskOwner, key: str) -> TaskSnapshot | None:
        record = idempotency_record(owner, self.server, key)
        rows = await self.store.query(
            query("runtime/_idempotent_task.surql"), {"id": record}
        )
        task = rows[0]
        if task is None:
            return None
        return await self.get(_record_uuid(task))

    async def _input_exchange_by_id(
        self, input_id: RecordID
    ) -> TaskInputExchange | None:
        rows = await self.store.query(
            query("runtime/_input_exchange_by_id.surql"), {"input": input_id}
        )
        record = rows[0]
        return _input_record_to_exchange(record) if record is not None else None

    async def _reset_for_recovery(self, task: TaskSnapshot) -> TaskSnapshot:
        return await self._force_status(
            task, TaskStatus.QUEUED, "reclaimed after process restart", None
        )

    async def _force_waiting(self, task: TaskSnapshot) -> TaskSnapshot:
        return await self._force_status(
            task, TaskStatus.WAITING, "waiting for provider webhook", None
        )

    async def _force_failed(
        self, task: TaskSnapshot, failure: TaskFailure
    ) -> TaskSnapshot:
        return await self._force_status(
            task, TaskStatus.FAILED, failure.message, failure
        )

    async def _force_status(
        self,
        task: TaskSnapshot,
        status: TaskStatus,
        message: str,
        failure: TaskFailure | None,
    ) -> TaskSnapshot:
        now = _now()
        envelope = {
            "input": task.request,
            "owner": task.owner.to_json(),
            "status_message": message,
            "ttl_ms": task.ttl_ms,
            "poll_interval_ms": task.poll_interval_ms,
        }
        terminal = status == TaskStatus.FAILED
        results = await self.store.query(
            query("runtime/_force_status.surql"),
            {
                "task": task_record(task.task_id),
                "status": status.value,
                "request": envelope,
                "error": failure.to_json() if failure is not None else None,
                "completed_at": now if terminal else None,
                "now": now,
                "expected": task.status.value,
                "expected_updated_at": task.updated_at,
            },
        )
        updated = results[2]
        if updated is None:
            raise Conflict(str(task.task_id))

        return _record_to_snapshot(updated)

    @staticmethod
    async def _recovery_result(operation: Any) -> TaskSnapshot | None:
        try:
            return await operation
        except Conflict:
            return None


def _record_uuid(record: Any) -> str:
    if isinstance(record, RecordID):
        return str(record.id)
    raise InvalidRecord(f"task id has non-record key: {record!r}")


def _record_to_snapshot(record: dict[str, Any]) -> TaskSnapshot:
    task_id = parse_task_id(_record_uuid(record["id"]))
    envelope = record["request"]
    owner = TaskOwner.from_json(envelope["owner"])
    tenant_key = owner.effective_tenant_key()
    if _record_uuid(record["tenant"]) != str(
        deterministic_tenant_id(tenant_key)
    ) or _record_uuid(record["owner"]) != str(
        deterministic_principal_id(tenant_key, owner.principal_key)
    ):
        raise InvalidRecord(
            "task owner references do not match its canonical platform identity"
        )
    error_value = record.get("error")
    error = (
        TaskFailure.from_json(error_value)
        if error_value is not None
        else None
    )
    result_value = record.get("result")
    server = record["server"]
    server_key = str(server.id) if isinstance(server, RecordID) else str(server)
    return TaskSnapshot(
        task_id=task_id,
        owner=owner,
        server=server_key,
        task_type=record["task_type"],
        request=envelope["input"],
        recovery_class=RecoveryClass(record["recovery_class"]),
        status=TaskStatus(record["status"]),
        status_message=envelope.get("status_message"),
        progress=record["progress"],
        result=task_result_from_store(result_value),
        error=error,
        idempotency_key=record.get("idempotency_key"),
        lease_owner=record.get("lease_owner"),
        lease_expires_at=record.get("lease_expires_at"),
        cancel_requested_at=record.get("cancel_requested_at"),
        created_at=record["created_at"],
        updated_at=record["updated_at"],
        started_at=record.get("started_at"),
        completed_at=record.get("completed_at"),
        retention_expires_at=record.get("retention_expires_at"),
        retention_pins=frozenset(record.get("retention_pins", [])),
        ttl_ms=envelope.get("ttl_ms"),
        poll_interval_ms=envelope.get("poll_interval_ms"),
    )


def _input_record_to_exchange(record: dict[str, Any]) -> TaskInputExchange:
    request = record["request"]
    return TaskInputExchange(
        key=record["request_key"],
        request=TaskInputRequest(
            method=request["method"], params=request.get("params", {})
        ),
        response=record.get("response"),
        created_at=record["created_at"],
        responded_at=record.get("responded_at"),
    )
