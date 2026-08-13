"""Thin Tasks extension binding for the official MCP Python SDK lifecycle."""

from __future__ import annotations

from collections.abc import AsyncIterator, Sequence
from dataclasses import dataclass
from typing import Any, Protocol

import mcp.types as types
from mcp.server import Server, ServerRequestContext
from mcp.server.context import CallNext, HandlerResult
from mcp.server.extension import Extension, MethodBinding, compose_tool_call_handler
from mcp.shared.exceptions import MCPError

from .models import (
    CANCEL_TASK_METHOD,
    EXTENSION_ID,
    GET_TASK_METHOD,
    PROTOCOL_VERSION,
    SUBSCRIPTION_ID_META_KEY,
    UPDATE_TASK_METHOD,
    AcknowledgeTaskResult,
    CancelTaskParams,
    CreateTaskResult,
    DetailedTask,
    GetTaskParams,
    GetTaskResult,
    TaskStatusNotification,
    TaskStatusNotificationParams,
    TaskSubscriptionFilter,
    TaskSubscriptionsListenParams,
    UpdateTaskParams,
)

Context = ServerRequestContext[Any, Any]


class TaskExtensionHandler(Protocol):
    def authenticate(self, ctx: Context) -> Any: ...

    async def start_tool_task(
        self, caller: Any, ctx: Context, request: types.CallToolRequestParams
    ) -> CreateTaskResult | None: ...

    async def get_task(
        self, caller: Any, ctx: Context, request: GetTaskParams
    ) -> GetTaskResult: ...

    async def update_task(
        self, caller: Any, ctx: Context, request: UpdateTaskParams
    ) -> AcknowledgeTaskResult: ...

    async def cancel_task(
        self, caller: Any, ctx: Context, request: CancelTaskParams
    ) -> AcknowledgeTaskResult: ...

    async def subscribe_tasks(
        self, caller: Any, ctx: Context, task_ids: Sequence[str]
    ) -> "TaskSubscription": ...


@dataclass
class TaskSubscription:
    accepted_task_ids: list[str]
    updates: AsyncIterator[DetailedTask]


class TasksExtension(Extension):
    """Official extension contribution plus the released SDK's missing Tasks types."""

    identifier = EXTENSION_ID

    def __init__(self, handler: TaskExtensionHandler) -> None:
        self.handler = handler

    def methods(self) -> Sequence[MethodBinding]:
        versions = frozenset({PROTOCOL_VERSION})
        return (
            MethodBinding(GET_TASK_METHOD, GetTaskParams, self._get_task, versions),
            MethodBinding(UPDATE_TASK_METHOD, UpdateTaskParams, self._update_task, versions),
            MethodBinding(CANCEL_TASK_METHOD, CancelTaskParams, self._cancel_task, versions),
        )

    async def intercept_tool_call(
        self,
        params: types.CallToolRequestParams,
        ctx: Context,
        call_next: CallNext,
    ) -> HandlerResult:
        if params.task is not None:
            raise MCPError(types.INVALID_PARAMS, "legacy task augmentation is not supported")
        if not _declares_tasks(ctx):
            return await call_next(ctx)
        caller = self.handler.authenticate(ctx)
        created = await self.handler.start_tool_task(caller, ctx, params)
        return created if created is not None else await call_next(ctx)

    async def _get_task(self, ctx: Context, params: GetTaskParams) -> dict[str, Any]:
        _require_tasks(ctx)
        result = await self.handler.get_task(self.handler.authenticate(ctx), ctx, params)
        return result.wire()

    async def _update_task(
        self, ctx: Context, params: UpdateTaskParams
    ) -> AcknowledgeTaskResult:
        _require_tasks(ctx)
        return await self.handler.update_task(
            self.handler.authenticate(ctx), ctx, params
        )

    async def _cancel_task(
        self, ctx: Context, params: CancelTaskParams
    ) -> AcknowledgeTaskResult:
        _require_tasks(ctx)
        return await self.handler.cancel_task(
            self.handler.authenticate(ctx), ctx, params
        )

    async def listen(
        self, ctx: Context, params: TaskSubscriptionsListenParams
    ) -> types.SubscriptionsListenResult:
        _require_tasks(ctx)
        requested = params.notifications.task_ids
        if requested is None:
            raise MCPError(
                types.INVALID_PARAMS,
                "subscriptions/listen requires notifications.taskIds",
            )
        if ctx.request_id is None:
            raise MCPError(types.INVALID_REQUEST, "subscription request id is missing")

        subscription = await self.handler.subscribe_tasks(
            self.handler.authenticate(ctx), ctx, requested
        )
        meta = {SUBSCRIPTION_ID_META_KEY: ctx.request_id}
        accepted = types.SubscriptionFilter.model_validate(
            {"taskIds": subscription.accepted_task_ids}
        )
        await ctx.session.send_notification(
            types.SubscriptionsAcknowledgedNotification(
                params=types.SubscriptionsAcknowledgedNotificationParams(
                    meta=meta,
                    notifications=accepted,
                )
            ),
            related_request_id=ctx.request_id,
        )
        async for task in subscription.updates:
            await ctx.session.send_notification(
                TaskStatusNotification(
                    params=TaskStatusNotificationParams(meta=meta, task=task)
                ),
                related_request_id=ctx.request_id,
            )
        return types.SubscriptionsListenResult(meta=meta)


def bind_tasks_extension(server: Server, extension: TasksExtension) -> None:
    """Bind Tasks without wrapping or reimplementing the SDK transport."""

    call_tool = server.get_request_handler("tools/call")
    if call_tool is None:
        raise ValueError("Tasks requires a tools/call handler")
    server.extensions[extension.identifier] = extension.settings()
    for binding in extension.methods():
        server.add_request_handler(binding.method, binding.params_type, binding.handler)
    server.add_request_handler(
        "tools/call",
        types.CallToolRequestParams,
        compose_tool_call_handler([extension], call_tool.handler),
    )
    server.add_request_handler(
        "subscriptions/listen", TaskSubscriptionsListenParams, extension.listen
    )


def _declares_tasks(ctx: Context) -> bool:
    capabilities = ctx.session.client_capabilities
    return bool(
        capabilities is not None
        and capabilities.extensions is not None
        and EXTENSION_ID in capabilities.extensions
    )


def _require_tasks(ctx: Context) -> None:
    if _declares_tasks(ctx):
        return
    raise MCPError(
        types.MISSING_REQUIRED_CLIENT_CAPABILITY,
        "missing required client capability",
        {"requiredCapabilities": {"extensions": {EXTENSION_ID: {}}}},
    )
