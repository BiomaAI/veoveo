"""Durable execution state for Veoveo MCP tasks, ported from `veoveo-task-runtime`.

SurrealDB is the sole task authority. LIVE queries may reduce latency, but
every read and transition is checked against durable state, and every state
transition emits an ordered outbox event in the same transaction.
"""

from __future__ import annotations

import asyncio
import uuid
from datetime import datetime, timedelta, timezone
from typing import Any, AsyncIterator

from surrealdb import RecordID

from .store import (
    MAX_TRANSACTION_ATTEMPTS,
    OutboxEvent,
    StoreError,
    SurrealStore,
    outbox_draft,
)
from .types import (
    EVENT_SCHEMA_VERSION,
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
    idempotency_record,
    parse_task_id,
    profile_record,
    server_record,
    task_input_record,
    task_record,
    validate_input_key,
    validate_input_method,
)

_OUTBOX_PAGE = 1_000
_WAKE_TIMEOUT_SECONDS = 2.0
_PAYLOAD_POLL_SECONDS = 0.5


def _now() -> datetime:
    return datetime.now(timezone.utc)


def _value_to_open_object(value: Any) -> dict[str, Any]:
    if isinstance(value, dict):
        return value
    return {"value": value}


def _open_object_to_value(value: dict[str, Any]) -> Any:
    if len(value) == 1 and "value" in value:
        return value["value"]
    return value


class TaskRuntime:
    def __init__(self, store: SurrealStore, server: str, worker_id: str) -> None:
        self.store = store
        self.server = server
        self.worker_id = worker_id
        self._workers: dict[uuid.UUID, tuple[asyncio.Event, asyncio.Task]] = {}
        self._changed = asyncio.Event()

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
            "status_message": "accepted; queued",
            "ttl_ms": draft.ttl_ms,
            "poll_interval_ms": draft.poll_interval_ms,
        }
        content = {
            "tenant": draft.owner.tenant_record(),
            "owner": draft.owner.principal_record(),
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
        initial_snapshot = TaskSnapshot(
            task_id=draft.task_id,
            owner=draft.owner,
            server=self.server,
            task_type=draft.task_type,
            request=draft.request,
            recovery_class=draft.recovery_class,
            status=TaskStatus.QUEUED,
            status_message="accepted; queued",
            progress=0.0,
            result=None,
            error=None,
            idempotency_key=draft.idempotency_key,
            lease_owner=None,
            lease_expires_at=None,
            cancel_requested_at=None,
            created_at=now,
            updated_at=now,
            started_at=None,
            completed_at=None,
            retention_expires_at=retention,
            retention_pins=frozenset(draft.retention_pins),
            ttl_ms=draft.ttl_ms,
            poll_interval_ms=draft.poll_interval_ms,
        )
        outbox = _task_event(initial_snapshot, "task.created")

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
                        "BEGIN TRANSACTION; CREATE ONLY $idempotency CONTENT $link "
                        "RETURN NONE; CREATE ONLY $task CONTENT $content RETURN NONE; "
                        "CREATE outbox_event CONTENT $outbox RETURN NONE; "
                        "COMMIT TRANSACTION;",
                        {
                            "idempotency": idempotency,
                            "link": link,
                            "task": record,
                            "content": content,
                            "outbox": outbox,
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
                "BEGIN TRANSACTION; CREATE ONLY $task CONTENT $content RETURN NONE; "
                "CREATE outbox_event CONTENT $outbox RETURN NONE; COMMIT TRANSACTION;",
                {"task": record, "content": content, "outbox": outbox},
            )

        snapshot = await self.get(str(draft.task_id))
        if snapshot is None:
            raise TaskNotFound(str(draft.task_id))
        self._note_change()
        return CreateTaskResult(snapshot=snapshot, created=True)

    async def get(self, task_id: str) -> TaskSnapshot | None:
        parsed = parse_task_id(task_id)
        rows = await self.store.query(
            "SELECT * FROM ONLY $task;", {"task": task_record(parsed)}
        )
        record = rows[0]
        if record is None:
            return None
        snapshot = _record_to_snapshot(record)
        return snapshot if snapshot.server == self.server else None

    async def list(self) -> list[TaskSnapshot]:
        rows = await self.store.query(
            "SELECT * FROM task WHERE server = $server ORDER BY created_at ASC;",
            {"server": server_record(self.server)},
        )
        return [_record_to_snapshot(record) for record in rows[0] or []]

    async def list_for_owner(self, owner: TaskOwner) -> list[TaskSnapshot]:
        rows = await self.store.query(
            "SELECT * FROM task WHERE server = $server AND tenant = $tenant AND "
            "owner = $owner AND profile = $profile ORDER BY created_at ASC;",
            {
                "server": server_record(self.server),
                "tenant": owner.tenant_record(),
                "owner": owner.principal_record(),
                "profile": profile_record(owner.profile),
            },
        )
        snapshots = [_record_to_snapshot(record) for record in rows[0] or []]
        return [
            snapshot
            for snapshot in snapshots
            if snapshot.owner.data_labels.issubset(owner.data_labels)
        ]

    async def owner(self, task_id: str) -> TaskOwner | None:
        snapshot = await self.get(task_id)
        return snapshot.owner if snapshot is not None else None

    async def acknowledge_retention_pin(self, task_id: str, pin: str) -> TaskSnapshot:
        parsed = parse_task_id(task_id)
        rows = await self.store.query(
            "UPDATE ONLY $task SET retention_pins -= $pin WHERE server = $server AND "
            "retention_pins CONTAINS $pin RETURN AFTER;",
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
            "status_message": "input required",
            "ttl_ms": current.ttl_ms,
            "poll_interval_ms": current.poll_interval_ms,
        }
        event_snapshot = _copy(current)
        event_snapshot.status = TaskStatus.WAITING
        event_snapshot.status_message = "input required"
        event_snapshot.updated_at = now
        event = _task_event(event_snapshot, "task.input_requested")
        try:
            await self.store.query(
                "BEGIN TRANSACTION; LET $updated = (UPDATE ONLY $task SET "
                "status = 'waiting', request = $request, updated_at = $now WHERE "
                "updated_at = $expected_updated_at AND status IN "
                "['queued', 'running', 'waiting'] AND server = $server AND "
                "tenant = $tenant AND owner = $owner AND lease_owner = $worker AND "
                "lease_expires_at > $now RETURN AFTER); IF $updated = NONE { THROW "
                "'task input transition conflict'; }; CREATE ONLY $input CONTENT "
                "$content RETURN NONE; CREATE outbox_event CONTENT $event RETURN NONE; "
                "COMMIT TRANSACTION;",
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
                    "event": event,
                },
            )
        except StoreError as error:
            if await self._input_exchange_by_id(input_id) is not None:
                raise DuplicateInputKey(key) from error
            recheck = await self.get(task_id)
            if recheck is None or recheck.updated_at != current.updated_at:
                raise Conflict(task_id) from error
            raise
        self._note_change()
        exchange = await self._input_exchange_by_id(input_id)
        if exchange is None:
            raise InvalidRecord("task input readback is missing")
        return exchange

    async def outstanding_inputs(self, task_id: str) -> dict[str, TaskInputRequest]:
        parsed = parse_task_id(task_id)
        if await self.get(str(parsed)) is None:
            raise TaskNotFound(str(parsed))
        rows = await self.store.query(
            "SELECT * FROM task_input WHERE task = $task AND response = NONE "
            "ORDER BY created_at ASC;",
            {"task": task_record(parsed)},
        )
        outstanding: dict[str, TaskInputRequest] = {}
        for record in rows[0] or []:
            exchange = _input_record_to_exchange(record)
            outstanding[exchange.key] = exchange.request
        return outstanding

    async def submit_input_responses(
        self, task_id: str, responses: dict[str, dict[str, Any]]
    ) -> TaskInputSubmission:
        current = await self.get(task_id)
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
                event_snapshot = _copy(current)
                event_snapshot.updated_at = now
                event = _task_event(event_snapshot, "task.input_received")
                try:
                    results = await self.store.query(
                        "BEGIN TRANSACTION; LET $updated = (UPDATE ONLY $input SET "
                        "response = $response, responded_at = $now WHERE task = $task "
                        "AND response = NONE RETURN AFTER); IF $updated != NONE { LET "
                        "$task_updated = (UPDATE ONLY $task SET updated_at = $now "
                        "WHERE server = $server AND status IN ['queued', 'running', "
                        "'waiting'] RETURN AFTER); IF $task_updated = NONE { THROW "
                        "'task cannot accept input'; }; CREATE outbox_event CONTENT "
                        "$event RETURN NONE; }; RETURN $updated; COMMIT TRANSACTION;",
                        {
                            "input": task_input_record(current.task_id, key),
                            "response": response_value,
                            "now": now,
                            "task": task_record(current.task_id),
                            "server": server_record(self.server),
                            "event": event,
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
                self._note_change()
            else:
                submission.ignored += 1
        return submission

    async def live_updates(self) -> AsyncIterator[TaskUpdate]:
        wake = await self.store.outbox_wake()
        cursor, snapshots = await self._update_baseline()
        return self._task_update_stream(wake, cursor, snapshots, False)

    async def live_updates_after(
        self, cursor: TaskUpdateCursor
    ) -> AsyncIterator[TaskUpdate]:
        wake = await self.store.outbox_wake()
        return self._task_update_stream(wake, cursor, [], True)

    async def _update_baseline(self) -> tuple[TaskUpdateCursor, list[TaskSnapshot]]:
        sequence = await self.store.latest_available_outbox_sequence()
        snapshots = await self.list()
        return TaskUpdateCursor(sequence), snapshots

    async def _task_update_stream(
        self,
        wake: Any,
        cursor: TaskUpdateCursor,
        initial: list[TaskSnapshot],
        replay_immediately: bool,
    ) -> AsyncIterator[TaskUpdate]:
        try:
            for snapshot in initial:
                yield TaskUpdate(cursor=cursor, snapshot=snapshot)
            must_replay = replay_immediately
            while True:
                if not must_replay:
                    await wake.wait(_WAKE_TIMEOUT_SECONDS)
                must_replay = False
                for update in await self._replay_task_updates(cursor):
                    cursor = update.cursor
                    yield update
        finally:
            await wake.close()

    async def _replay_task_updates(
        self, cursor: TaskUpdateCursor
    ) -> list[TaskUpdate]:
        updates: list[TaskUpdate] = []
        sequence = cursor.sequence
        while True:
            events = await self.store.read_outbox(sequence, _OUTBOX_PAGE)
            for event in events:
                sequence = event.sequence
                if event.aggregate_type != "task":
                    continue
                snapshot = _task_snapshot_from_event(event)
                if snapshot.server == self.server:
                    updates.append(
                        TaskUpdate(
                            cursor=TaskUpdateCursor(sequence), snapshot=snapshot
                        )
                    )
            if len(events) < _OUTBOX_PAGE:
                break
        return updates

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
        event_snapshot = _copy(snapshot)
        event_snapshot.status = TaskStatus.RUNNING
        event_snapshot.status_message = "claimed for execution"
        event_snapshot.lease_owner = self.worker_id
        event_snapshot.lease_expires_at = lease_expires_at
        event_snapshot.started_at = snapshot.started_at or now
        event_snapshot.updated_at = now
        event = _task_event(event_snapshot, "task.claimed")
        envelope = {
            "input": snapshot.request,
            "owner": snapshot.owner.to_json(),
            "status_message": "claimed for execution",
            "ttl_ms": snapshot.ttl_ms,
            "poll_interval_ms": snapshot.poll_interval_ms,
        }
        results = await self.store.query(
            "BEGIN TRANSACTION; LET $updated = (UPDATE ONLY $task SET "
            "status = 'running', request = $request, lease_owner = $worker, "
            "lease_expires_at = $lease_expires, started_at = started_at ?? $now, "
            "updated_at = $now WHERE status = $expected AND updated_at = "
            "$expected_updated_at AND (lease_expires_at = NONE OR lease_expires_at "
            "<= $now OR lease_owner = $worker) RETURN AFTER); IF $updated != NONE { "
            "CREATE outbox_event CONTENT $event RETURN NONE; }; RETURN $updated; "
            "COMMIT TRANSACTION;",
            {
                "task": task_record(snapshot.task_id),
                "worker": self.worker_id,
                "request": envelope,
                "lease_expires": lease_expires_at,
                "now": now,
                "expected": snapshot.status.value,
                "expected_updated_at": snapshot.updated_at,
                "event": event,
            },
        )
        updated = results[3]
        if updated is None:
            raise Conflict(task_id)
        self._note_change()
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
            "UPDATE ONLY $task SET lease_expires_at = $lease_expires WHERE "
            "lease_owner = $worker AND lease_expires_at > $now AND status IN "
            "['running', 'waiting', 'cancel_requested'] RETURN AFTER;",
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
        self, current: TaskSnapshot, transition: TaskTransition
    ) -> TaskSnapshot:
        task_id = str(current.task_id)
        if current.server != self.server:
            raise WrongServer(task_id)
        durable = await self.get(task_id)
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
        event_snapshot = _transitioned_snapshot(durable, transition, now)
        event = _task_event(event_snapshot, f"task.{next_status.value}")
        result = transition.result()
        failure = transition.failure()
        results = await self.store.query(
            "BEGIN TRANSACTION; LET $updated = (UPDATE ONLY $task SET status = $next, "
            "request = $request, progress = $progress, result = $result, "
            "error = $error, cancel_requested_at = $cancel_requested_at, "
            "completed_at = $completed_at, lease_owner = IF $terminal { NONE } ELSE { "
            "lease_owner }, lease_expires_at = IF $terminal { NONE } ELSE { "
            "lease_expires_at }, updated_at = $now WHERE status = $expected AND "
            "updated_at = $expected_updated_at AND server = $server AND "
            "tenant = $tenant AND owner = $owner AND ($control_transition OR "
            "(lease_owner = $worker AND lease_expires_at > $now) OR "
            "($expired_cancellation AND (lease_expires_at = NONE OR lease_expires_at "
            "<= $now))) RETURN AFTER); IF $updated != NONE { CREATE outbox_event "
            "CONTENT $event RETURN NONE; }; RETURN $updated; COMMIT TRANSACTION;",
            {
                "task": task_record(current.task_id),
                "next": next_status.value,
                "request": envelope,
                "progress": progress,
                "result": _value_to_open_object(result) if result is not None else None,
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
                "event": event,
            },
        )
        updated = results[3]
        if updated is None:
            raise Conflict(task_id)
        self._note_change()
        return _record_to_snapshot(updated)

    async def cancel(self, task_id: str) -> TaskSnapshot:
        while True:
            current = await self.get(task_id)
            if current is None:
                raise TaskNotFound(task_id)
            if current.is_terminal():
                return current
            if current.status == TaskStatus.CANCEL_REQUESTED:
                requested = current
            else:
                try:
                    requested = await self.transition_if_current(
                        current, TaskTransition.cancel_requested()
                    )
                except Conflict:
                    continue
            worker = self._workers.get(requested.task_id)
            if worker is not None:
                worker[0].set()
            if requested.lease_owner is None:
                try:
                    return await self.transition_if_current(
                        requested, TaskTransition.cancelled()
                    )
                except Conflict:
                    continue
            return requested

    async def is_cancel_requested(self, task_id: str) -> bool:
        snapshot = await self.get(task_id)
        return snapshot is not None and snapshot.status == TaskStatus.CANCEL_REQUESTED

    async def await_terminal(self, task_id: str) -> TaskSnapshot:
        """Poll the durable row until the task leaves execution states."""
        while True:
            snapshot = await self.get(task_id)
            if snapshot is None:
                raise TaskNotFound(task_id)
            if snapshot.is_terminal():
                return snapshot
            self._changed.clear()
            try:
                await asyncio.wait_for(self._changed.wait(), _PAYLOAD_POLL_SECONDS)
            except TimeoutError:
                pass

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
            "BEGIN TRANSACTION; LET $expired = (SELECT VALUE id FROM task WHERE "
            "retention_expires_at != NONE AND retention_expires_at <= $now AND "
            "array::len(retention_pins) = 0 AND status IN ['succeeded', 'failed', "
            "'cancelled']); DELETE task_idempotency WHERE task IN $expired RETURN "
            "NONE; DELETE task_input WHERE task IN $expired RETURN NONE; LET "
            "$deleted = (DELETE task WHERE id IN $expired RETURN BEFORE); RETURN "
            "$deleted; COMMIT TRANSACTION;",
            {"now": _now()},
        )
        return [_record_to_snapshot(record).task_id for record in results[5] or []]

    async def _idempotent_task(self, owner: TaskOwner, key: str) -> TaskSnapshot | None:
        record = idempotency_record(owner, self.server, key)
        rows = await self.store.query(
            "SELECT VALUE task FROM ONLY $id;", {"id": record}
        )
        task = rows[0]
        if task is None:
            return None
        return await self.get(_record_uuid(task))

    async def _input_exchange_by_id(
        self, input_id: RecordID
    ) -> TaskInputExchange | None:
        rows = await self.store.query(
            "SELECT * FROM ONLY $input;", {"input": input_id}
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
        event_snapshot = _copy(task)
        event_snapshot.status = status
        event_snapshot.status_message = message
        event_snapshot.error = failure
        event_snapshot.lease_owner = None
        event_snapshot.lease_expires_at = None
        event_snapshot.completed_at = now if terminal else None
        event_snapshot.updated_at = now
        event = _task_event(event_snapshot, f"task.{status.value}")
        results = await self.store.query(
            "BEGIN TRANSACTION; LET $updated = (UPDATE ONLY $task SET "
            "status = $status, request = $request, error = $error, "
            "lease_owner = NONE, lease_expires_at = NONE, completed_at = "
            "$completed_at, updated_at = $now WHERE status = $expected AND "
            "updated_at = $expected_updated_at AND (lease_expires_at = NONE OR "
            "lease_expires_at <= $now) RETURN AFTER); IF $updated != NONE { CREATE "
            "outbox_event CONTENT $event RETURN NONE; }; RETURN $updated; "
            "COMMIT TRANSACTION;",
            {
                "task": task_record(task.task_id),
                "status": status.value,
                "request": envelope,
                "error": failure.to_json() if failure is not None else None,
                "completed_at": now if terminal else None,
                "now": now,
                "expected": task.status.value,
                "expected_updated_at": task.updated_at,
                "event": event,
            },
        )
        updated = results[3]
        if updated is None:
            raise Conflict(str(task.task_id))
        self._note_change()
        return _record_to_snapshot(updated)

    @staticmethod
    async def _recovery_result(operation: Any) -> TaskSnapshot | None:
        try:
            return await operation
        except Conflict:
            return None

    def _note_change(self) -> None:
        self._changed.set()


def _copy(snapshot: TaskSnapshot) -> TaskSnapshot:
    import copy

    return copy.copy(snapshot)


def _transitioned_snapshot(
    current: TaskSnapshot, transition: TaskTransition, now: datetime
) -> TaskSnapshot:
    snapshot = _copy(current)
    next_status = transition.status()
    terminal = next_status.is_terminal()
    snapshot.status = next_status
    snapshot.status_message = transition.message()
    snapshot.progress = transition.progress(current.progress)
    snapshot.result = transition.result()
    snapshot.error = transition.failure()
    if next_status == TaskStatus.CANCEL_REQUESTED:
        snapshot.cancel_requested_at = now
    snapshot.completed_at = now if terminal else None
    snapshot.updated_at = now
    if terminal:
        snapshot.lease_owner = None
        snapshot.lease_expires_at = None
    return snapshot


def _task_event(snapshot: TaskSnapshot, event_type: str) -> dict[str, Any]:
    return outbox_draft(
        snapshot.owner.tenant_record(),
        "task",
        str(snapshot.task_id),
        event_type,
        EVENT_SCHEMA_VERSION,
        {"snapshot": snapshot.to_json()},
    )


def _task_snapshot_from_event(event: OutboxEvent) -> TaskSnapshot:
    if event.schema_version != EVENT_SCHEMA_VERSION:
        raise InvalidRecord(
            f"task outbox event {event.sequence} has schema version "
            f"{event.schema_version}, expected {EVENT_SCHEMA_VERSION}"
        )
    snapshot = TaskSnapshot.from_json(event.payload["snapshot"])
    if event.aggregate_id != str(snapshot.task_id):
        raise InvalidRecord(
            f"task outbox event {event.sequence} aggregate id does not match its snapshot"
        )
    return snapshot


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
        TaskFailure.from_json(_open_object_to_value(error_value))
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
        result=_open_object_to_value(result_value) if result_value is not None else None,
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
