"""Principal, tenant, and label ownership for datasheet tasks."""

from __future__ import annotations

from typing import Any

from mcp.shared.exceptions import McpError
from mcp.types import INVALID_REQUEST, ErrorData

from veoveo_mcp.contract.identity import GatewayInternalIdentity, PlaneCaller
from veoveo_mcp.internal_auth import BEARER_SCOPE_KEY, IDENTITY_SCOPE_KEY
from veoveo_mcp.tasks import PrincipalKind, TaskOwner

from .app_state import AppState


def _invalid(message: str) -> McpError:
    return McpError(ErrorData(code=INVALID_REQUEST, message=message))


def identity_from_scope(scope: dict[str, Any]) -> GatewayInternalIdentity:
    identity = scope.get(IDENTITY_SCOPE_KEY)
    if identity is None:
        raise _invalid("gateway identity missing")
    return identity


def caller_from_scope(scope: dict[str, Any]) -> PlaneCaller:
    identity = identity_from_scope(scope)
    bearer = scope.get(BEARER_SCOPE_KEY)
    if bearer is None:
        raise _invalid("forwarded bearer missing")
    return PlaneCaller.from_identity(identity, bearer)


def request_scope(server: Any) -> dict[str, Any]:
    """The ASGI scope of the HTTP request behind the current MCP call."""
    request = server.request_context.request
    if request is None:
        raise _invalid("authenticated HTTP context missing")
    return request.scope


def runtime_owner(identity: GatewayInternalIdentity) -> TaskOwner:
    actor = identity.actor
    return TaskOwner(
        principal_key=actor.id,
        principal_kind=PrincipalKind(actor.kind.value),
        issuer=actor.issuer,
        subject=actor.subject,
        profile=identity.profile,
        tenant_key=actor.tenant,
        authority=identity.authority,
        data_labels=frozenset(actor.data_labels),
    )


def task_owner_allows(owner: TaskOwner, identity: GatewayInternalIdentity) -> bool:
    caller = runtime_owner(identity)
    return owner.allows(
        caller.principal_key,
        caller.profile,
        caller.tenant_key,
        caller.data_labels,
    )


async def require_task_owner(
    state: AppState, identity: GatewayInternalIdentity, task_id: str
) -> None:
    owner = await state.tasks.owner(task_id)
    if owner is None:
        raise _invalid("task ownership record missing")
    if not task_owner_allows(owner, identity):
        raise _invalid("datasheet task policy denied request")
