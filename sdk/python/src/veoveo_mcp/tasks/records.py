"""Closed database records for the Task owner's snapshot and request envelope."""

from datetime import datetime
from typing import Annotated, Literal

from surrealdb import RecordID

from pydantic import (
    BaseModel, ConfigDict, Field, JsonValue, StrictStr, ValidationInfo,
    field_validator, model_validator,
)

from ..contract.identity import (
    AccessLevel, DataLabelId, GatewayProfileId, InvocationAuthority, PolicyVersion,
    PrincipalId, PrincipalKind, TenantId, TokenIssuer, TokenSubject, WorkContextId,
    DelegationId,
    WorkContextMembershipLevel,
)
from .types import (
    CreateTask, RecoveryClass, TaskFailure, TaskInputRequest, TaskOwner,
    TaskSnapshot, TaskStatus, profile_record, server_record,
)


class OwnerContextRecord(BaseModel):
    model_config = ConfigDict(extra="forbid", frozen=True)
    principal_key: PrincipalId
    principal_kind: PrincipalKind
    issuer: TokenIssuer
    subject: TokenSubject
    profile: GatewayProfileId
    tenant_key: TenantId | None = None
    data_labels: frozenset[DataLabelId]
    authority: InvocationAuthority

    @classmethod
    def from_owner(cls, owner: TaskOwner) -> "OwnerContextRecord":
        return cls.model_validate(owner.to_json())

    def to_native(self) -> dict[str, object]:
        from .store import _json_to_surreal
        value = self.model_dump(mode="json")
        value["data_labels"] = sorted(self.data_labels)
        value["authority"]["output_policy"]["data_labels"] = sorted(self.authority.output_policy.data_labels)
        return _json_to_surreal(value)

    def to_owner(self) -> TaskOwner:
        return TaskOwner.from_json(self.model_dump(mode="json"))


class TaskRequestRecord(BaseModel):
    model_config = ConfigDict(extra="forbid", frozen=True, allow_inf_nan=False)
    @model_validator(mode="before")
    @classmethod
    def decode_native_json(cls, value: object, info: ValidationInfo) -> object:
        if info.context and info.context.get("native"):
            from .store import _json_from_surreal
            from .types import InvalidRecord
            try:
                return _json_from_surreal(value)
            except InvalidRecord as error:
                raise ValueError(str(error)) from error
        return value

    input: JsonValue
    status_message: str | None = None
    ttl_ms: Annotated[int, Field(strict=True, ge=0, le=2**64 - 1)] | None = None
    poll_interval_ms: Annotated[int, Field(strict=True, ge=0, le=2**64 - 1)] | None = None

    @classmethod
    def from_snapshot(cls, snapshot: TaskSnapshot, *, status_message: str | None) -> "TaskRequestRecord":
        return cls(input=snapshot.request, status_message=status_message,
                   ttl_ms=snapshot.ttl_ms, poll_interval_ms=snapshot.poll_interval_ms)

    def to_native(self) -> dict[str, object]:
        from .store import _json_to_surreal
        return _json_to_surreal(self.model_dump(mode="json"))


class _GrantProjection(BaseModel):
    model_config = ConfigDict(extra="forbid", frozen=True)
    subject_kind: Literal["principal", "group"]
    subject_key: str
    permission: AccessLevel


class AuthorityRecord(BaseModel):
    model_config = ConfigDict(extra="forbid", frozen=True)
    context_key: WorkContextId
    membership: WorkContextMembershipLevel
    policy_revision: PolicyVersion
    owner_kind: Literal["principal", "group"]
    owner_key: str
    initial_grants: tuple[_GrantProjection, ...] = ()
    classification: DataLabelId | None = None
    data_labels: frozenset[DataLabelId] = frozenset()
    invocation_mode: Literal["direct", "delegated", "automated"]
    initiator_key: PrincipalId | None = None
    delegation_id: DelegationId | None = None

    @classmethod
    def from_owner(cls, owner: TaskOwner) -> "AuthorityRecord":
        authority = owner.authority
        output = authority.output_policy
        return cls(
            context_key=authority.work_context, membership=authority.membership,
            policy_revision=authority.policy_revision,
            owner_kind=output.owner.kind, owner_key=output.owner.id,
            initial_grants=tuple(_GrantProjection(subject_kind=grant.subject.kind,
                                subject_key=grant.subject.id, permission=grant.level)
                                for grant in output.initial_grants),
            classification=output.classification, data_labels=output.data_labels,
            invocation_mode=authority.invocation_mode, initiator_key=authority.initiator,
            delegation_id=authority.delegation_id,
        )

    def to_native(self) -> dict[str, object]:
        value = self.model_dump(mode="json")
        value["data_labels"] = sorted(self.data_labels)
        return value


class TaskFailureRecord(BaseModel):
    model_config = ConfigDict(extra="forbid", frozen=True, allow_inf_nan=False)
    code: StrictStr
    message: StrictStr
    details: JsonValue = None

    @classmethod
    def from_failure(cls, failure: TaskFailure) -> "TaskFailureRecord":
        return cls.model_validate(failure.to_json())

    def to_native(self) -> dict[str, object]:
        from .store import _json_to_surreal
        return _json_to_surreal(self.model_dump(mode="json", exclude_unset=True))


class TaskInputRequestRecord(BaseModel):
    model_config = ConfigDict(extra="forbid", frozen=True, allow_inf_nan=False)
    method: StrictStr
    params: dict[str, JsonValue]

    @field_validator("method")
    @classmethod
    def admit_method(cls, method: str) -> str:
        from .types import validate_input_method
        validate_input_method(method)
        return method

    @classmethod
    def from_request(cls, request: TaskInputRequest) -> "TaskInputRequestRecord":
        return cls(method=request.method, params=request.params)

    def to_native(self) -> dict[str, object]:
        from .store import _json_to_surreal
        return _json_to_surreal(self.model_dump(mode="json"))


class TaskCreateRecord(BaseModel):
    """Native CREATE content; references and datetimes never pass through JSON."""
    model_config = ConfigDict(extra="forbid", frozen=True, arbitrary_types_allowed=True)
    tenant: RecordID
    owner: RecordID
    work_context: RecordID
    initiator: RecordID | None
    invocation_mode: Literal["direct", "delegated", "automated"]
    delegation_id: DelegationId | None
    policy_revision: PolicyVersion
    authority: AuthorityRecord
    profile: RecordID
    server: RecordID
    task_type: StrictStr
    status: TaskStatus
    recovery_class: RecoveryClass
    request: TaskRequestRecord
    owner_context: OwnerContextRecord
    progress: float = 0.0
    result: None = None
    error: None = None
    result_artifact: None = None
    idempotency_key: str | None
    lease_owner: None = None
    lease_expires_at: None = None
    cancel_requested_at: None = None
    created_at: datetime
    updated_at: datetime
    started_at: None = None
    completed_at: None = None
    retention_expires_at: datetime
    retention_pins: tuple[str, ...]
    search_text: str

    @classmethod
    def from_draft(cls, draft: CreateTask, *, work_context: RecordID,
                   initiator: RecordID | None, now: datetime,
                   retention_expires_at: datetime) -> "TaskCreateRecord":
        owner = draft.owner
        return cls(
            tenant=owner.tenant_record(), owner=owner.principal_record(),
            work_context=work_context, initiator=initiator,
            invocation_mode=owner.authority.invocation_mode,
            delegation_id=owner.authority.delegation_id,
            policy_revision=owner.authority.policy_revision,
            authority=AuthorityRecord.from_owner(owner),
            profile=profile_record(owner.profile), server=server_record(draft.server),
            task_type=draft.task_type, status=TaskStatus.QUEUED,
            recovery_class=draft.recovery_class,
            request=TaskRequestRecord(input=draft.request, status_message="Queued",
                                      ttl_ms=draft.ttl_ms, poll_interval_ms=draft.poll_interval_ms),
            owner_context=OwnerContextRecord.from_owner(owner),
            idempotency_key=draft.idempotency_key, created_at=now, updated_at=now,
            retention_expires_at=retention_expires_at,
            retention_pins=tuple(sorted(draft.retention_pins)),
            search_text=f"{draft.server} {draft.task_type} {owner.principal_key}",
        )

    def to_native(self) -> dict[str, object]:
        value = self.model_dump(mode="python")
        value.update(status=self.status.value, recovery_class=self.recovery_class.value,
                     authority=self.authority.to_native(), request=self.request.to_native(),
                     owner_context=self.owner_context.to_native(), retention_pins=list(self.retention_pins))
        return value


class TaskIdempotencyRecord(BaseModel):
    model_config = ConfigDict(extra="forbid", frozen=True, arbitrary_types_allowed=True)
    task: RecordID
    tenant: RecordID
    owner: RecordID
    server: RecordID
    key: str
    created_at: datetime

    @classmethod
    def from_draft(cls, draft: CreateTask, task: RecordID, now: datetime) -> "TaskIdempotencyRecord":
        if draft.idempotency_key is None:
            raise ValueError("idempotency record requires a key")
        return cls(task=task, tenant=draft.owner.tenant_record(), owner=draft.owner.principal_record(),
                   server=server_record(draft.server), key=draft.idempotency_key, created_at=now)

    def to_native(self) -> dict[str, object]:
        return self.model_dump(mode="python")


class TaskInputRecord(BaseModel):
    model_config = ConfigDict(extra="forbid", frozen=True, arbitrary_types_allowed=True)
    task: RecordID
    request_key: str
    request: TaskInputRequestRecord
    response: None = None
    created_at: datetime
    responded_at: None = None

    @classmethod
    def from_request(cls, task: RecordID, key: str, request: TaskInputRequest, now: datetime) -> "TaskInputRecord":
        return cls(task=task, request_key=key, request=TaskInputRequestRecord.from_request(request), created_at=now)

    def to_native(self) -> dict[str, object]:
        value = self.model_dump(mode="python")
        value["request"] = self.request.to_native()
        return value
