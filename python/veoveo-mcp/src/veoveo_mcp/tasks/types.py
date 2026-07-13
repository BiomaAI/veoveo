"""Durable task vocabulary, ported from the Rust `veoveo-task-runtime` crate.

Every JSON shape here (snapshots, owners, failures, outbox payloads) must stay
byte-compatible with the Rust serde output: agents and the console consume
these records regardless of which language wrote them.
"""

from __future__ import annotations

import hashlib
import uuid
from dataclasses import dataclass, field as dataclass_field
from datetime import datetime, timedelta, timezone
from enum import Enum
from typing import Any

from surrealdb import RecordID

PLATFORM_ID_NAMESPACE = uuid.UUID("7f7b11e2-3b9a-5c7a-9d51-2cf8e1bdfab4")
INSTALLATION_TENANT = "installation"
DEFAULT_RETENTION = timedelta(days=7)
EVENT_SCHEMA_VERSION = 2


class TaskStatus(str, Enum):
    QUEUED = "queued"
    RUNNING = "running"
    WAITING = "waiting"
    SUCCEEDED = "succeeded"
    FAILED = "failed"
    CANCEL_REQUESTED = "cancel_requested"
    CANCELLED = "cancelled"

    def is_terminal(self) -> bool:
        return self in (TaskStatus.SUCCEEDED, TaskStatus.FAILED, TaskStatus.CANCELLED)


class RecoveryClass(str, Enum):
    RESUME = "resume"
    WEBHOOK_WAIT = "webhook_wait"
    INTERRUPTED_INDETERMINATE = "interrupted_indeterminate"


class PrincipalKind(str, Enum):
    USER = "user"
    SERVICE = "service"


class TaskError(Exception):
    pass


class TaskNotFound(TaskError):
    def __init__(self, task_id: str) -> None:
        super().__init__(f"task `{task_id}` was not found")
        self.task_id = task_id


class WrongServer(TaskError):
    def __init__(self, task_id: str) -> None:
        super().__init__(f"task `{task_id}` is not owned by this server")


class InvalidTransition(TaskError):
    def __init__(self, from_status: TaskStatus, to_status: TaskStatus) -> None:
        super().__init__(
            f"task transition from {from_status.value} to {to_status.value} is not allowed"
        )
        self.from_status = from_status
        self.to_status = to_status


class Conflict(TaskError):
    def __init__(self, task_id: str) -> None:
        super().__init__(f"task `{task_id}` changed concurrently")
        self.task_id = task_id


class LeaseHeld(TaskError):
    def __init__(self, task_id: str) -> None:
        super().__init__(f"task `{task_id}` has an active lease")
        self.task_id = task_id


class InvalidProgress(TaskError):
    def __init__(self) -> None:
        super().__init__("task progress must be finite and in 0..=1")


class InvalidRecord(TaskError):
    def __init__(self, message: str) -> None:
        super().__init__(f"invalid persisted task: {message}")


class InvalidInputKey(TaskError):
    def __init__(self) -> None:
        super().__init__(
            "task input key is empty, too long, or contains a control character"
        )


class DuplicateInputKey(TaskError):
    def __init__(self, key: str) -> None:
        super().__init__(f"task input key `{key}` has already been used")


def rfc3339(value: datetime) -> str:
    if value.tzinfo is None:
        raise InvalidRecord("datetime is missing a timezone")
    value = value.astimezone(timezone.utc)
    return value.strftime("%Y-%m-%dT%H:%M:%S.%f") + "Z"


def parse_rfc3339(value: str) -> datetime:
    return datetime.fromisoformat(value.replace("Z", "+00:00"))


def validate_retention_pin(value: str) -> str:
    if not value or len(value) > 256 or any(ch < " " or ch == "\x7f" for ch in value):
        raise InvalidRecord(
            "task retention pin is empty, too long, or contains a control character"
        )
    return value


def deterministic_tenant_id(tenant_key: str) -> uuid.UUID:
    _validate_identity_field("tenant_key", tenant_key, 256)
    return uuid.uuid5(PLATFORM_ID_NAMESPACE, f"tenant:{tenant_key}")


def deterministic_principal_id(tenant_key: str, principal_key: str) -> uuid.UUID:
    _validate_identity_field("tenant_key", tenant_key, 256)
    _validate_identity_field("principal_key", principal_key, 512)
    return uuid.uuid5(PLATFORM_ID_NAMESPACE, f"principal:{tenant_key}:{principal_key}")


def deterministic_enterprise_id() -> uuid.UUID:
    return uuid.uuid5(PLATFORM_ID_NAMESPACE, "veoveo-installation")


def _validate_identity_field(name: str, value: str, max_bytes: int) -> None:
    if not value.strip():
        raise InvalidRecord(f"identity field `{name}` must not be empty")
    if len(value.encode()) > max_bytes:
        raise InvalidRecord(f"identity field `{name}` exceeds maximum encoded length")
    if any(ch < " " or ch == "\x7f" for ch in value):
        raise InvalidRecord(
            f"identity field `{name}` must not contain control characters"
        )


@dataclass(frozen=True)
class TaskOwner:
    principal_key: str
    principal_kind: PrincipalKind
    issuer: str
    subject: str
    profile: str
    tenant_key: str | None
    data_labels: frozenset[str] = dataclass_field(default_factory=frozenset)

    def effective_tenant_key(self) -> str:
        return self.tenant_key if self.tenant_key is not None else INSTALLATION_TENANT

    def allows(
        self,
        principal_key: str,
        profile: str,
        tenant_key: str | None,
        data_labels: frozenset[str] | set[str],
    ) -> bool:
        return (
            self.principal_key == principal_key
            and self.profile == profile
            and self.tenant_key == tenant_key
            and self.data_labels.issubset(data_labels)
        )

    def to_json(self) -> dict[str, Any]:
        return {
            "principal_key": self.principal_key,
            "principal_kind": self.principal_kind.value,
            "issuer": self.issuer,
            "subject": self.subject,
            "profile": self.profile,
            "tenant_key": self.tenant_key,
            "data_labels": sorted(self.data_labels),
        }

    @classmethod
    def from_json(cls, value: dict[str, Any]) -> "TaskOwner":
        return cls(
            principal_key=value["principal_key"],
            principal_kind=PrincipalKind(value["principal_kind"]),
            issuer=value["issuer"],
            subject=value["subject"],
            profile=value["profile"],
            tenant_key=value.get("tenant_key"),
            data_labels=frozenset(value.get("data_labels", [])),
        )

    def tenant_record(self) -> RecordID:
        return RecordID("tenant", deterministic_tenant_id(self.effective_tenant_key()))

    def principal_record(self) -> RecordID:
        return RecordID(
            "principal",
            deterministic_principal_id(self.effective_tenant_key(), self.principal_key),
        )


@dataclass(frozen=True)
class TaskFailure:
    code: str
    message: str
    details: Any | None = None

    def to_json(self) -> dict[str, Any]:
        value: dict[str, Any] = {"code": self.code, "message": self.message}
        if self.details is not None:
            value["details"] = self.details
        return value

    @classmethod
    def from_json(cls, value: dict[str, Any]) -> "TaskFailure":
        return cls(
            code=value["code"],
            message=value["message"],
            details=value.get("details"),
        )

    @classmethod
    def interrupted_indeterminate(cls) -> "TaskFailure":
        return cls(
            "interrupted_indeterminate",
            "execution was interrupted; commit state is indeterminate and the task "
            "will not be replayed",
        )


@dataclass
class CreateTask:
    task_id: uuid.UUID
    owner: TaskOwner
    server: str
    task_type: str
    request: Any
    recovery_class: RecoveryClass
    idempotency_key: str | None = None
    ttl_ms: int | None = None
    poll_interval_ms: int | None = None
    retention_pins: frozenset[str] = dataclass_field(default_factory=frozenset)


@dataclass
class TaskSnapshot:
    task_id: uuid.UUID
    owner: TaskOwner
    server: str
    task_type: str
    request: Any
    recovery_class: RecoveryClass
    status: TaskStatus
    status_message: str | None
    progress: float
    result: Any | None
    error: TaskFailure | None
    idempotency_key: str | None
    lease_owner: str | None
    lease_expires_at: datetime | None
    cancel_requested_at: datetime | None
    created_at: datetime
    updated_at: datetime
    started_at: datetime | None
    completed_at: datetime | None
    retention_expires_at: datetime | None
    retention_pins: frozenset[str]
    ttl_ms: int | None
    poll_interval_ms: int | None

    def is_terminal(self) -> bool:
        return self.status.is_terminal()

    def to_json(self) -> dict[str, Any]:
        def opt_time(value: datetime | None) -> str | None:
            return rfc3339(value) if value is not None else None

        return {
            "task_id": str(self.task_id),
            "owner": self.owner.to_json(),
            "server": self.server,
            "task_type": self.task_type,
            "request": self.request,
            "recovery_class": self.recovery_class.value,
            "status": self.status.value,
            "status_message": self.status_message,
            "progress": self.progress,
            "result": self.result,
            "error": self.error.to_json() if self.error is not None else None,
            "idempotency_key": self.idempotency_key,
            "lease_owner": self.lease_owner,
            "lease_expires_at": opt_time(self.lease_expires_at),
            "cancel_requested_at": opt_time(self.cancel_requested_at),
            "created_at": rfc3339(self.created_at),
            "updated_at": rfc3339(self.updated_at),
            "started_at": opt_time(self.started_at),
            "completed_at": opt_time(self.completed_at),
            "retention_expires_at": opt_time(self.retention_expires_at),
            "retention_pins": sorted(self.retention_pins),
            "ttl_ms": self.ttl_ms,
            "poll_interval_ms": self.poll_interval_ms,
        }

    @classmethod
    def from_json(cls, value: dict[str, Any]) -> "TaskSnapshot":
        def opt_time(name: str) -> datetime | None:
            raw = value.get(name)
            return parse_rfc3339(raw) if raw is not None else None

        error = value.get("error")
        return cls(
            task_id=parse_task_id(value["task_id"]),
            owner=TaskOwner.from_json(value["owner"]),
            server=value["server"],
            task_type=value["task_type"],
            request=value["request"],
            recovery_class=RecoveryClass(value["recovery_class"]),
            status=TaskStatus(value["status"]),
            status_message=value.get("status_message"),
            progress=value["progress"],
            result=value.get("result"),
            error=TaskFailure.from_json(error) if error is not None else None,
            idempotency_key=value.get("idempotency_key"),
            lease_owner=value.get("lease_owner"),
            lease_expires_at=opt_time("lease_expires_at"),
            cancel_requested_at=opt_time("cancel_requested_at"),
            created_at=parse_rfc3339(value["created_at"]),
            updated_at=parse_rfc3339(value["updated_at"]),
            started_at=opt_time("started_at"),
            completed_at=opt_time("completed_at"),
            retention_expires_at=opt_time("retention_expires_at"),
            retention_pins=frozenset(value.get("retention_pins", [])),
            ttl_ms=value.get("ttl_ms"),
            poll_interval_ms=value.get("poll_interval_ms"),
        )


@dataclass
class CreateTaskResult:
    snapshot: TaskSnapshot
    created: bool


@dataclass
class ClaimedTask:
    snapshot: TaskSnapshot
    lease_owner: str
    lease_expires_at: datetime


@dataclass
class TaskInputRequest:
    method: str
    params: dict[str, Any] = dataclass_field(default_factory=dict)


@dataclass
class TaskInputExchange:
    key: str
    request: TaskInputRequest
    response: dict[str, Any] | None
    created_at: datetime
    responded_at: datetime | None


@dataclass
class TaskInputSubmission:
    accepted: int = 0
    ignored: int = 0


@dataclass
class RecoveryReport:
    resumable: list[TaskSnapshot] = dataclass_field(default_factory=list)
    webhook_waiting: list[TaskSnapshot] = dataclass_field(default_factory=list)
    failed_indeterminate: list[TaskSnapshot] = dataclass_field(default_factory=list)
    cancelled: list[TaskSnapshot] = dataclass_field(default_factory=list)


@dataclass(frozen=True)
class TaskUpdateCursor:
    sequence: int = 0

    def __post_init__(self) -> None:
        if self.sequence < 0:
            raise InvalidRecord("outbox sequence is negative")


@dataclass
class TaskUpdate:
    cursor: TaskUpdateCursor
    snapshot: TaskSnapshot


class TaskTransition:
    """One of Running/Waiting/Succeeded/Failed/CancelRequested/Cancelled."""

    def __init__(
        self,
        status: TaskStatus,
        message: str,
        progress: float | None = None,
        result: Any | None = None,
        failure: TaskFailure | None = None,
    ) -> None:
        self._status = status
        self._message = message
        self._progress = progress
        self._result = result
        self._failure = failure

    @classmethod
    def running(cls, message: str, progress: float) -> "TaskTransition":
        return cls(TaskStatus.RUNNING, message, progress=progress)

    @classmethod
    def waiting(cls, message: str, progress: float) -> "TaskTransition":
        return cls(TaskStatus.WAITING, message, progress=progress)

    @classmethod
    def succeeded(cls, message: str, result: Any) -> "TaskTransition":
        return cls(TaskStatus.SUCCEEDED, message, result=result)

    @classmethod
    def failed(cls, failure: TaskFailure) -> "TaskTransition":
        return cls(TaskStatus.FAILED, failure.message, failure=failure)

    @classmethod
    def cancel_requested(cls) -> "TaskTransition":
        return cls(TaskStatus.CANCEL_REQUESTED, "cancellation requested")

    @classmethod
    def cancelled(cls) -> "TaskTransition":
        return cls(TaskStatus.CANCELLED, "cancelled")

    def status(self) -> TaskStatus:
        return self._status

    def message(self) -> str:
        return self._message

    def progress(self, current: float) -> float:
        if self._progress is not None:
            return self._progress
        if self._status == TaskStatus.SUCCEEDED:
            return 1.0
        return current

    def result(self) -> Any | None:
        return self._result

    def failure(self) -> TaskFailure | None:
        return self._failure


def allowed_transition(from_status: TaskStatus, to_status: TaskStatus) -> bool:
    if from_status == TaskStatus.QUEUED:
        return to_status in (
            TaskStatus.RUNNING,
            TaskStatus.WAITING,
            TaskStatus.CANCEL_REQUESTED,
            TaskStatus.FAILED,
        )
    if from_status == TaskStatus.RUNNING:
        return to_status in (
            TaskStatus.RUNNING,
            TaskStatus.WAITING,
            TaskStatus.SUCCEEDED,
            TaskStatus.FAILED,
            TaskStatus.CANCEL_REQUESTED,
        )
    if from_status == TaskStatus.WAITING:
        return to_status in (
            TaskStatus.RUNNING,
            TaskStatus.SUCCEEDED,
            TaskStatus.FAILED,
            TaskStatus.CANCEL_REQUESTED,
        )
    if from_status == TaskStatus.CANCEL_REQUESTED:
        return to_status in (TaskStatus.CANCELLED, TaskStatus.FAILED)
    return False


def parse_task_id(value: str | uuid.UUID) -> uuid.UUID:
    try:
        parsed = value if isinstance(value, uuid.UUID) else uuid.UUID(value)
    except ValueError as error:
        raise InvalidRecord(str(error)) from error
    if parsed.version != 7:
        raise InvalidRecord("task id must be a UUIDv7")
    return parsed


def task_record(task_id: uuid.UUID) -> RecordID:
    return RecordID("task", task_id)


def server_record(server: str) -> RecordID:
    return RecordID("mcp_server", server)


def profile_record(profile: str) -> RecordID:
    return RecordID("profile", profile)


def idempotency_record(owner: TaskOwner, server: str, key: str) -> RecordID:
    digest = hashlib.sha256(
        "\0".join(
            [owner.effective_tenant_key(), owner.principal_key, owner.profile, server, key]
        ).encode()
    ).hexdigest()
    return RecordID("task_idempotency", digest)


def task_input_record(task_id: uuid.UUID, key: str) -> RecordID:
    digest = hashlib.sha256(f"{task_id}\0{key}".encode()).hexdigest()
    return RecordID("task_input", digest)


def validate_input_key(key: str) -> None:
    if not key or len(key) > 256 or any(ch < " " or ch == "\x7f" for ch in key):
        raise InvalidInputKey()


def validate_input_method(method: str) -> None:
    if not method or len(method) > 256 or any(ch < " " or ch == "\x7f" for ch in method):
        raise InvalidRecord(
            "task input method is empty, too long, or contains a control character"
        )


def new_task_id() -> uuid.UUID:
    import uuid_extensions

    return uuid_extensions.uuid7()


def default_retention_expiry(
    now: datetime, ttl_ms: int | None
) -> datetime | None:
    if ttl_ms is not None:
        return now + timedelta(milliseconds=ttl_ms)
    return now + DEFAULT_RETENTION
