"""Contract tests for the thin MCP 2026-07-28 Tasks binding."""

from datetime import datetime, timezone
from types import SimpleNamespace
from typing import Any

import mcp.types as types
import pytest
from mcp.server import Server
from mcp.shared.exceptions import MCPError

from veoveo_mcp.task_extension import (
    EXTENSION_ID,
    PROTOCOL_VERSION,
    AcknowledgeTaskResult,
    CreateTaskResult,
    GetTaskResult,
    Task,
    TasksExtension,
    TaskSubscription,
    WorkingTask,
    bind_tasks_extension,
)


def working(task_id: str = "provider/opaque-task") -> WorkingTask:
    now = datetime.now(timezone.utc)
    return WorkingTask(
        task_id=task_id,
        status_message="working",
        created_at=now,
        last_updated_at=now,
        ttl_ms=60_000,
        poll_interval_ms=3_000,
    )


class FakeSession:
    def __init__(self, with_tasks: bool = True) -> None:
        extensions = {EXTENSION_ID: {}} if with_tasks else {}
        self.client_capabilities = types.ClientCapabilities(extensions=extensions)
        self.sent: list[tuple[Any, Any]] = []

    async def send_notification(self, notification, related_request_id=None) -> None:
        self.sent.append((notification, related_request_id))


def context(with_tasks: bool = True):
    return SimpleNamespace(session=FakeSession(with_tasks), request_id="listen-1")


class FakeHandler:
    def authenticate(self, _ctx):
        return "caller"

    async def start_tool_task(self, _caller, _ctx, request):
        if request.name != "forecast":
            return None
        seed = working()
        return CreateTaskResult.from_task(Task(**seed.model_dump()))

    async def get_task(self, _caller, _ctx, request):
        return GetTaskResult(task=working(request.task_id))

    async def update_task(self, _caller, _ctx, _request):
        return AcknowledgeTaskResult()

    async def cancel_task(self, _caller, _ctx, _request):
        return AcknowledgeTaskResult()

    async def subscribe_tasks(self, _caller, _ctx, task_ids):
        async def updates():
            yield working(task_ids[0])

        return TaskSubscription(list(task_ids), updates())


async def ordinary_call(_ctx, _params):
    return types.CallToolResult(
        content=[types.TextContent(type="text", text="ordinary")]
    )


def server() -> tuple[Server, TasksExtension]:
    instance = Server("tasks-test", on_call_tool=ordinary_call, on_ping=None)
    extension = TasksExtension(FakeHandler())
    bind_tasks_extension(instance, extension)
    return instance, extension


def test_binding_advertises_tasks_and_registers_final_methods():
    instance, _ = server()
    assert PROTOCOL_VERSION == "2026-07-28"
    assert instance.extensions == {EXTENSION_ID: {}}
    assert instance.get_request_handler("tasks/get") is not None
    assert instance.get_request_handler("tasks/update") is not None
    assert instance.get_request_handler("tasks/cancel") is not None
    assert instance.get_request_handler("subscriptions/listen") is not None
    assert instance.get_request_handler("ping") is None


async def test_task_creation_is_per_request_capability_gated():
    _, extension = server()
    params = types.CallToolRequestParams(name="forecast", arguments={})

    async def fallback(_ctx):
        return {"resultType": "complete", "ordinary": True}

    created = await extension.intercept_tool_call(params, context(), fallback)
    assert created.result_type == "task"
    assert created.task_id == "provider/opaque-task"

    direct = await extension.intercept_tool_call(
        params, context(with_tasks=False), fallback
    )
    assert direct == {"resultType": "complete", "ordinary": True}


async def test_task_methods_require_capability_and_emit_flattened_results():
    _, extension = server()
    result = await extension._get_task(
        context(), SimpleNamespace(task_id="upstream:opaque")
    )
    assert result["resultType"] == "complete"
    assert result["taskId"] == "upstream:opaque"
    assert "task" not in result

    with pytest.raises(MCPError) as caught:
        await extension._get_task(
            context(with_tasks=False), SimpleNamespace(task_id="task")
        )
    assert caught.value.code == types.MISSING_REQUIRED_CLIENT_CAPABILITY


async def test_task_subscription_uses_the_request_scoped_channel():
    _, extension = server()
    ctx = context()
    params = SimpleNamespace(
        notifications=SimpleNamespace(task_ids=["provider/opaque-task"])
    )
    result = await extension.listen(ctx, params)
    assert result.result_type == "complete"
    assert len(ctx.session.sent) == 2
    assert [related for _, related in ctx.session.sent] == ["listen-1", "listen-1"]
    acknowledged = ctx.session.sent[0][0].model_dump(
        by_alias=True, mode="json", exclude_none=True
    )
    assert acknowledged["params"]["notifications"]["taskIds"] == [
        "provider/opaque-task"
    ]
    update = ctx.session.sent[1][0].model_dump(
        by_alias=True, mode="json", exclude_none=True
    )
    assert update["method"] == "notifications/tasks"
    assert update["params"]["taskId"] == "provider/opaque-task"
