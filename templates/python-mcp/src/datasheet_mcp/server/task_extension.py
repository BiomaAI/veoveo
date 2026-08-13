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
from veoveo_mcp.tasks import TaskError, TaskSnapshot

from ..contract import ProfileDatasetRequest
from .app_state import AppState
from .ownership import request_scope, runtime_owner
from .profile_task import ProfileTaskError, start_profile_task

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

    def authenticate(self, ctx: Context) -> AuthenticatedCaller:
        scope = request_scope(ctx)
        identity = scope.get(IDENTITY_SCOPE_KEY)
        if identity is None:
            raise MCPError(types.INVALID_REQUEST, "gateway identity missing")
        bearer = scope.get(BEARER_SCOPE_KEY)
        if bearer is None:
            raise MCPError(types.INVALID_REQUEST, "forwarded bearer missing")
        return AuthenticatedCaller(
            identity=identity,
            plane=PlaneCaller.from_identity(identity, bearer),
        )

    async def _authorized_snapshot(
        self, caller: AuthenticatedCaller, task_id: str
    ) -> TaskSnapshot:
        try:
            snapshot = await self.state.tasks.get(task_id)
        except TaskError as error:
            raise _internal(str(error)) from error
        if snapshot is None:
            raise _invalid("unknown task id")
        owner = runtime_owner(caller.identity)
        if snapshot.owner.allows(
            owner.principal_key,
            owner.profile,
            owner.tenant_key,
            owner.data_labels,
        ):
            return snapshot
        raise _invalid("unknown task id")

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
            task = await project_snapshot(self.state.tasks, snapshot)
        except TaskError as error:
            raise _internal(str(error)) from error
        return GetTaskResult(task=task)

    async def update_task(
        self, caller: AuthenticatedCaller, _ctx: Context, request: UpdateTaskParams
    ) -> AcknowledgeTaskResult:
        await self._authorized_snapshot(caller, request.task_id)
        try:
            await self.state.tasks.submit_input_responses(
                request.task_id, request.input_responses
            )
        except TaskError as error:
            raise _internal(str(error)) from error
        return AcknowledgeTaskResult()

    async def cancel_task(
        self, caller: AuthenticatedCaller, _ctx: Context, request: CancelTaskParams
    ) -> AcknowledgeTaskResult:
        await self._authorized_snapshot(caller, request.task_id)
        try:
            await self.state.tasks.cancel(request.task_id)
        except TaskError as error:
            raise _internal(str(error)) from error
        return AcknowledgeTaskResult()

    async def subscribe_tasks(
        self, caller: AuthenticatedCaller, _ctx: Context, task_ids: Sequence[str]
    ) -> TaskSubscription:
        accepted: list[str] = []
        for task_id in task_ids:
            try:
                await self._authorized_snapshot(caller, task_id)
            except MCPError:
                continue
            accepted.append(task_id)
        accepted_keys = set(accepted)
        caller_owner = runtime_owner(caller.identity)
        try:
            updates = await self.state.tasks.live_updates()
        except TaskError as error:
            raise _internal(str(error)) from error

        async def stream():
            async for update in updates:
                snapshot = update.snapshot
                if str(snapshot.task_id) not in accepted_keys:
                    continue
                if not snapshot.owner.allows(
                    caller_owner.principal_key,
                    caller_owner.profile,
                    caller_owner.tenant_key,
                    caller_owner.data_labels,
                ):
                    continue
                yield await project_snapshot(self.state.tasks, snapshot)

        return TaskSubscription(accepted_task_ids=accepted, updates=stream())
