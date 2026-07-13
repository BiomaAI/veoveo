"""Final task extension adapter over the datasheet TaskRuntime."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Sequence

from pydantic import ValidationError

from veoveo_mcp.contract.identity import GatewayInternalIdentity, PlaneCaller
from veoveo_mcp.internal_auth import BEARER_SCOPE_KEY, IDENTITY_SCOPE_KEY
from veoveo_mcp.task_extension import (
    AcknowledgeTaskResult,
    AdapterError,
    CancelTaskParams,
    CreateTaskResult,
    GetTaskParams,
    GetTaskResult,
    TaskSubscription,
    ToolCallParams,
    UpdateTaskParams,
    project_snapshot,
    task_seed,
)
from veoveo_mcp.tasks import TaskError, TaskSnapshot

from ..contract import ProfileDatasetRequest
from .app_state import AppState
from .ownership import runtime_owner
from .profile_task import ProfileTaskError, start_profile_task


@dataclass
class AuthenticatedCaller:
    identity: GatewayInternalIdentity
    plane: PlaneCaller


class DatasheetTaskExtension:
    def __init__(self, state: AppState) -> None:
        self.state = state

    def authenticate(self, scope: dict[str, Any]) -> AuthenticatedCaller:
        identity = scope.get(IDENTITY_SCOPE_KEY)
        if identity is None:
            raise AdapterError.unauthorized("gateway identity missing")
        bearer = scope.get(BEARER_SCOPE_KEY)
        if bearer is None:
            raise AdapterError.unauthorized("forwarded bearer missing")
        return AuthenticatedCaller(
            identity=identity,
            plane=PlaneCaller.from_identity(identity, bearer),
        )

    async def _authorized_snapshot(
        self, caller: AuthenticatedCaller, task_id: Any
    ) -> TaskSnapshot:
        try:
            snapshot = await self.state.tasks.get(str(task_id))
        except TaskError as error:
            raise AdapterError.internal(str(error)) from error
        if snapshot is None:
            raise AdapterError.invalid_params("unknown task id")
        caller_owner = runtime_owner(caller.identity)
        if snapshot.owner.allows(
            caller_owner.principal_key,
            caller_owner.profile,
            caller_owner.tenant_key,
            caller_owner.data_labels,
        ):
            return snapshot
        raise AdapterError.invalid_params("unknown task id")

    async def start_tool_task(
        self, caller: AuthenticatedCaller, request: ToolCallParams
    ) -> CreateTaskResult | None:
        if request.name != "profile_dataset":
            return None
        retention_pins = (
            frozenset([request.meta.task_retention_pin])
            if request.meta.task_retention_pin is not None
            else frozenset()
        )
        try:
            args = ProfileDatasetRequest.model_validate(request.arguments)
        except ValidationError as error:
            raise AdapterError.invalid_params(str(error)) from error
        try:
            snapshot = await start_profile_task(
                self.state, caller.identity, caller.plane, args, retention_pins
            )
        except (ProfileTaskError, TaskError) as error:
            raise AdapterError.internal(str(error)) from error
        return CreateTaskResult.from_task(task_seed(snapshot))

    async def get_task(
        self, caller: AuthenticatedCaller, request: GetTaskParams
    ) -> GetTaskResult:
        snapshot = await self._authorized_snapshot(caller, request.task_id)
        try:
            task = await project_snapshot(self.state.tasks, snapshot)
        except TaskError as error:
            raise AdapterError.internal(str(error)) from error
        return GetTaskResult(task=task)

    async def update_task(
        self, caller: AuthenticatedCaller, request: UpdateTaskParams
    ) -> AcknowledgeTaskResult:
        await self._authorized_snapshot(caller, request.task_id)
        try:
            await self.state.tasks.submit_input_responses(
                str(request.task_id), request.input_responses
            )
        except TaskError as error:
            raise AdapterError.internal(str(error)) from error
        return AcknowledgeTaskResult()

    async def cancel_task(
        self, caller: AuthenticatedCaller, request: CancelTaskParams
    ) -> AcknowledgeTaskResult:
        await self._authorized_snapshot(caller, request.task_id)
        try:
            await self.state.tasks.cancel(str(request.task_id))
        except TaskError as error:
            raise AdapterError.internal(str(error)) from error
        return AcknowledgeTaskResult()

    async def subscribe_tasks(
        self, caller: AuthenticatedCaller, task_ids: Sequence[Any]
    ) -> TaskSubscription:
        accepted = []
        for task_id in task_ids:
            try:
                await self._authorized_snapshot(caller, task_id)
            except AdapterError:
                continue
            accepted.append(task_id)
        accepted_keys = {str(task_id) for task_id in accepted}
        caller_owner = runtime_owner(caller.identity)
        try:
            updates = await self.state.tasks.live_updates()
        except TaskError as error:
            raise AdapterError.internal(str(error)) from error

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
