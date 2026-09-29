"""Official Tasks extension adapter over the datasheet durable runtime."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Sequence

import mcp.types as types
from mcp.server import ServerRequestContext
from mcp.shared.exceptions import MCPError
from pydantic import TypeAdapter, ValidationError

from veoveo_mcp.contract.identity import GatewayInternalIdentity, PlaneCaller
from veoveo_mcp.internal_auth import BEARER_SCOPE_KEY, IDENTITY_SCOPE_KEY
from veoveo_mcp.task_extension import (
    TASK_RETENTION_PIN_META_KEY,
    AcknowledgeTaskResult,
    CancelTaskParams,
    CreateTaskResult,
    GetTaskParams,
    GetTaskResult,
    TaskRetentionPin,
    TaskSubscription,
    UpdateTaskParams,
    project_snapshot,
    task_seed,
)
from veoveo_mcp.tasks import OwnerTaskQuery, TaskError, TaskNotFound, TaskSnapshot, parse_task_id
from veoveo_mcp.task_extension.projection import ProjectedTaskUpdates

from ..contract import ProfileDatasetRequest
from .app_state import AppState
from .ownership import GATEWAY_ROUTING_REQUIRED, request_scope, runtime_owner
from .profile_task import TASK_TYPE, ProfileTaskError, start_profile_task

Context = ServerRequestContext[Any, Any]
_retention_pin = TypeAdapter(TaskRetentionPin)


@dataclass
class AuthenticatedCaller:
    identity: GatewayInternalIdentity
    plane: PlaneCaller


def _invalid(message: str) -> MCPError:
    return MCPError(types.INVALID_PARAMS, message)


def _internal(message: str) -> MCPError:
    return MCPError(types.INTERNAL_ERROR, message)


class DatasheetTaskExtension:
    def __init__(self, state: AppState) -> None:
        self.state = state

    def _query(self, caller: AuthenticatedCaller) -> OwnerTaskQuery:
        return self.state.tasks.for_owner(runtime_owner(caller.identity)).of_type(TASK_TYPE)

    def authenticate(self, ctx: Context) -> AuthenticatedCaller:
        scope = request_scope(ctx)
        identity = scope.get(IDENTITY_SCOPE_KEY)
        if identity is None:
            raise MCPError(types.INVALID_REQUEST, GATEWAY_ROUTING_REQUIRED)
        bearer = scope.get(BEARER_SCOPE_KEY)
        if bearer is None:
            raise MCPError(types.INVALID_REQUEST, GATEWAY_ROUTING_REQUIRED)
        return AuthenticatedCaller(
            identity=identity,
            plane=PlaneCaller.from_identity(identity, bearer),
        )

    async def _authorized_snapshot(
        self, caller: AuthenticatedCaller, task_id: str
    ) -> TaskSnapshot:
        try:
            snapshot = await self._query(caller).get(parse_task_id(task_id))
        except TaskError as error:
            raise _internal(str(error)) from error
        if snapshot is None:
            raise _invalid("unknown task id")
        return snapshot

    async def start_tool_task(
        self,
        caller: AuthenticatedCaller,
        _ctx: Context,
        request: types.CallToolRequestParams,
    ) -> CreateTaskResult | None:
        if request.name != "profile_dataset":
            return None
        raw_pin = (request.meta or {}).get(TASK_RETENTION_PIN_META_KEY)
        try:
            pin = _retention_pin.validate_python(raw_pin) if raw_pin is not None else None
            args = ProfileDatasetRequest.model_validate(request.arguments or {})
        except ValidationError as error:
            raise _invalid(str(error)) from error
        retention_pins = frozenset([pin]) if pin is not None else frozenset()
        try:
            snapshot = await start_profile_task(
                self.state, caller.identity, caller.plane, args, retention_pins
            )
        except (ProfileTaskError, TaskError) as error:
            raise _internal(str(error)) from error
        return CreateTaskResult.from_task(task_seed(snapshot))

    async def get_task(
        self, caller: AuthenticatedCaller, _ctx: Context, request: GetTaskParams
    ) -> GetTaskResult:
        snapshot = await self._authorized_snapshot(caller, request.task_id)
        try:
            task = await project_snapshot(self._query(caller), snapshot)
        except TaskError as error:
            raise _internal(str(error)) from error
        return GetTaskResult(task=task)

    async def update_task(
        self, caller: AuthenticatedCaller, _ctx: Context, request: UpdateTaskParams
    ) -> AcknowledgeTaskResult:
        try:
            await self._query(caller).submit_input_responses(
                parse_task_id(request.task_id), request.input_responses
            )
        except TaskNotFound as error:
            raise _invalid("unknown task id") from error
        except TaskError as error:
            raise _internal(str(error)) from error
        return AcknowledgeTaskResult()

    async def cancel_task(
        self, caller: AuthenticatedCaller, _ctx: Context, request: CancelTaskParams
    ) -> AcknowledgeTaskResult:
        try:
            await self._query(caller).cancel(parse_task_id(request.task_id))
        except TaskNotFound as error:
            raise _invalid("unknown task id") from error
        except TaskError as error:
            raise _internal(str(error)) from error
        return AcknowledgeTaskResult()

    async def subscribe_tasks(
        self, caller: AuthenticatedCaller, _ctx: Context, task_ids: Sequence[str]
    ) -> TaskSubscription:
        if len(task_ids) > 256:
            raise _invalid("Task subscription accepts at most 256 identities")
        ids = []
        for task_id in task_ids:
            try:
                ids.append(parse_task_id(task_id))
            except TaskError:
                continue
        query = self._query(caller)
        try:
            subscription = await query.subscribe(ids)
        except TaskError as error:
            raise _internal(str(error)) from error

        return TaskSubscription(
            accepted_task_ids=[str(task_id) for task_id in subscription.accepted_task_ids],
            updates=ProjectedTaskUpdates(query, subscription.updates),
        )
