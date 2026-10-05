"""Closed database records for the Task owner's snapshot and request envelope."""

from typing import Annotated, Literal

from pydantic import BaseModel, ConfigDict, Field, JsonValue, StrictStr, ValidationInfo, field_validator, model_validator

from ..contract.identity import (
    AccessLevel, DataLabelId, GatewayProfileId, InvocationAuthority, PolicyVersion,
    PrincipalId, PrincipalKind, TenantId, TokenIssuer, TokenSubject, WorkContextId,
    WorkContextMembershipLevel,
)
from .types import TaskOwner


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
    delegation_id: str | None = None


class TaskFailureRecord(BaseModel):
    model_config = ConfigDict(extra="forbid", frozen=True, allow_inf_nan=False)
    code: StrictStr
    message: StrictStr
    details: JsonValue = None


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
