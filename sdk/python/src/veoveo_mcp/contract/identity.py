"""Gateway-signed identity vocabulary shared with the Rust `mcp-contract` crate.

These models parse the claims embedded in gateway internal tokens. The gateway
alone mints them; Python servers only verify and consume.
"""

from __future__ import annotations

from datetime import datetime
from enum import Enum
from typing import Annotated, Any

from pydantic import AfterValidator, BaseModel, ConfigDict, Field

GATEWAY_INTERNAL_TOKEN_ISSUER = "veoveo-internal"
DEFAULT_GATEWAY_INTERNAL_SIGNING_KEY_ID = "veoveo-internal-1"


def _identifier(max_bytes: int) -> Any:
    def validate(value: str) -> str:
        if (
            not value
            or value.strip() != value
            or len(value.encode()) > max_bytes
            or any(ch for ch in value if ch < " " or ch == "\x7f")
        ):
            raise ValueError(
                "identifier must be trimmed, non-empty, bounded, and printable"
            )
        return value

    return AfterValidator(validate)


TokenIssuer = Annotated[str, _identifier(2_048)]
TokenSubject = Annotated[str, _identifier(2_048)]
PrincipalId = Annotated[str, _identifier(1_024)]
TenantId = Annotated[str, _identifier(256)]
GroupId = Annotated[str, _identifier(512)]
RoleId = Annotated[str, _identifier(256)]
ScopeName = Annotated[str, _identifier(256)]
DataLabelId = Annotated[str, _identifier(256)]
GatewayProfileId = Annotated[str, _identifier(256)]
ServerSlug = Annotated[str, _identifier(128)]
JwtId = Annotated[str, _identifier(512)]


class PrincipalKind(str, Enum):
    USER = "user"
    SERVICE = "service"


class GroupRole(str, Enum):
    READ = "read"
    WRITE = "write"
    ADMIN = "admin"


class GroupMembership(BaseModel):
    model_config = ConfigDict(frozen=True)

    group: GroupId
    role: GroupRole


class Principal(BaseModel):
    model_config = ConfigDict(extra="allow")

    id: PrincipalId
    kind: PrincipalKind
    issuer: TokenIssuer
    subject: TokenSubject
    tenant: TenantId | None = None
    groups: set[GroupId] = Field(default_factory=set)
    group_roles: set[GroupMembership] = Field(default_factory=set)
    roles: set[RoleId] = Field(default_factory=set)
    scopes: set[ScopeName] = Field(default_factory=set)
    data_labels: set[DataLabelId] = Field(default_factory=set)
    assurances: set[str] = Field(default_factory=set)
    authenticated_at: datetime | None = None

    def group_memberships(self) -> set[GroupMembership]:
        """Effective `(group, role)` set: explicit roles win, bare membership reads."""
        explicit = {membership.group: membership.role for membership in self.group_roles}
        return {
            GroupMembership(group=group, role=explicit.get(group, GroupRole.READ))
            for group in self.groups
        }


class GatewayInternalIdentity(BaseModel):
    issuer: TokenIssuer
    profile: GatewayProfileId
    server: ServerSlug
    principal: Principal
    jwt_id: JwtId
    issued_at: datetime
    not_before: datetime
    expires_at: datetime


class PlaneCaller(BaseModel):
    """What a domain server presents when acting on a principal's behalf.

    The server has already verified the incoming gateway token; it forwards the
    same bearer to the artifact plane and carries the parsed identity for local
    reasoning.
    """

    model_config = ConfigDict(arbitrary_types_allowed=True)

    bearer_token: str
    identity: GatewayInternalIdentity
    memberships: set[GroupMembership] = Field(default_factory=set)

    def __repr__(self) -> str:  # never leak the bearer
        return f"PlaneCaller(identity={self.identity!r}, bearer_token=<redacted>)"

    @classmethod
    def from_identity(
        cls, identity: GatewayInternalIdentity, bearer_token: str
    ) -> "PlaneCaller":
        return cls(
            bearer_token=bearer_token,
            identity=identity,
            memberships=identity.principal.group_memberships(),
        )
