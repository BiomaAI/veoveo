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


@pytest.mark.parametrize("fail_after", [0, 1])
async def test_subscription_closes_reader_when_acknowledgement_or_delivery_fails(fail_after):
    class Updates:
        closed = False

        def __aiter__(self):
            return self

        async def __anext__(self):
            return working()

        async def aclose(self):
            self.closed = True

    updates = Updates()

    class Handler(FakeHandler):
        async def subscribe_tasks(self, _caller, _ctx, task_ids):
            return TaskSubscription(list(task_ids), updates)

    extension = TasksExtension(Handler())
    ctx = context()

    async def send(_notification, related_request_id=None):
        if len(ctx.session.sent) == fail_after:
            raise ConnectionError("client disconnected")
        ctx.session.sent.append((_notification, related_request_id))

    ctx.session.send_notification = send
    with pytest.raises(ConnectionError):
        await extension.listen(ctx, SimpleNamespace(
            notifications=SimpleNamespace(task_ids=["provider/opaque-task"]),
        ))
    assert updates.closed


@pytest.mark.parametrize("status", ["working", "input_required", "completed", "failed", "cancelled"])
def test_flattened_task_receiver_admits_current_fields_and_refuses_retired_mixed(status):
    from pydantic import ValidationError
    now = "2026-10-08T00:00:00Z"
    value = {"resultType": "complete", "taskId": "provider/opaque-task", "status": status,
             "statusMessage": "current", "createdAt": now, "lastUpdatedAt": now,
             "ttlMs": 60000, "pollIntervalMs": 3000,
             "_meta": {"owner.extension": {"snake_payload": True}}}
    if status == "input_required":
        value["inputRequests"] = {"open": {"method": "elicitation/create", "params": {
            "mode": "form", "message": "choose", "requestedSchema": {"type": "object", "properties": {
                "snake_payload": {"type": "string"}}}}}}
    elif status == "completed":
        value["result"] = {"provider_owned": {"snake_payload": True}}
    elif status == "failed":
        value["error"] = {"provider_owned": {"snake_payload": True}}
    assert GetTaskResult.from_wire(value).wire() == value
    for current, retired in [("taskId", "task_id"), ("statusMessage", "status_message"),
                             ("createdAt", "created_at"), ("lastUpdatedAt", "last_updated_at"),
                             ("ttlMs", "ttl_ms"), ("pollIntervalMs", "poll_interval_ms")]:
        renamed = dict(value)
        renamed[retired] = renamed.pop(current)
        for bad in [renamed, {**value, retired: value[current]}]:
            with pytest.raises(ValidationError):
                GetTaskResult.from_wire(bad)
    for bad in [{**value, "foreignField": 1}, {k: v for k, v in value.items() if k != "taskId"}]:
        with pytest.raises(ValidationError):
            GetTaskResult.from_wire(bad)
    if status == "input_required":
        for bad in [{**value, "input_requests": value["inputRequests"]},
                    {**{k: v for k, v in value.items() if k != "inputRequests"},
                     "input_requests": value["inputRequests"]}]:
            with pytest.raises(ValidationError):
                GetTaskResult.from_wire(bad)


@pytest.mark.parametrize("method,body", [
    ("tasks/get", {"taskId": "provider/opaque-task"}),
    ("tasks/cancel", {"taskId": "provider/opaque-task"}),
    ("tasks/update", {"taskId": "provider/opaque-task", "inputResponses": {
        "roots": {"roots": [{"uri": "file:///tmp", "name": "fixture"}]}}}),
    ("subscriptions/listen", {"notifications": {"taskIds": ["provider/opaque-task"],
        "resourcesListChanged": True, "ownerExtension": {"snake_payload": True}}}),
])
async def test_actual_bound_task_params_admit_aliases_before_handler(method, body):
    from pydantic import ValidationError
    instance, _ = server()
    entry = instance.get_request_handler(method)
    # The maintained ServerRunner uses this exact params type with by_name=False.
    current = {**body, "_meta": {"owner.extension": {"snake_payload": True},
        "io.modelcontextprotocol/protocolVersion": PROTOCOL_VERSION,
        "io.modelcontextprotocol/clientCapabilities": {"extensions": {EXTENSION_ID: {}}}}}
    admitted = entry.params_type.model_validate(current, by_name=False)
    assert admitted.meta["owner.extension"]["snake_payload"] is True
    from mcp.server.runner import ServerRunner
    sent = []

    async def notify(name, params):
        sent.append((name, params))

    channel = SimpleNamespace(request_id="request-1", message_metadata=None, notify=notify)
    connection = SimpleNamespace(protocol_version=PROTOCOL_VERSION, initialize_accepted=True,
        client_capabilities=types.ClientCapabilities(extensions={EXTENSION_ID: {}}), outbound=channel)
    runner = ServerRunner(instance, connection, None)
    result = await runner.on_request(channel, method, current)
    assert result is not None
    if method == "subscriptions/listen":
        assert [name for name, _ in sent] == ["notifications/subscriptions/acknowledged", "notifications/tasks"]
    bad = {**current, "foreignField": 1}
    with pytest.raises(ValidationError):
        await runner.on_request(channel, method, bad)
    if method == "subscriptions/listen":
        assert admitted.notifications.model_extra["ownerExtension"] == {"snake_payload": True}
        for canonical, retired in [("taskIds", "task_ids"), ("resourcesListChanged", "resources_list_changed")]:
            old = dict(body["notifications"])
            old[retired] = old.pop(canonical)
            for fields in [old, {**body["notifications"], retired: body["notifications"][canonical]}]:
                with pytest.raises((ValueError, ValidationError)):
                    await runner.on_request(channel, method, {**current, "notifications": fields})
    else:
        for canonical, retired in [("taskId", "task_id")] + ([("inputResponses", "input_responses")] if method == "tasks/update" else []):
            old = dict(body)
            old[retired] = old.pop(canonical)
            for fields in [old, {**body, retired: body[canonical]}]:
                with pytest.raises(ValidationError):
                    await runner.on_request(channel, method, {**fields, "_meta": current["_meta"]})


def test_internal_task_constructors_and_external_json_admission_are_distinct():
    import json
    from pydantic import ValidationError
    from veoveo_mcp.task_extension.models import GetTaskParams, TaskSubscriptionFilter
    seed = working()
    assert Task(task_id=seed.task_id, status="working", created_at=seed.created_at,
                last_updated_at=seed.last_updated_at).task_id == seed.task_id
    assert GetTaskParams(task_id=seed.task_id).task_id == seed.task_id
    assert GetTaskParams.model_validate_json(json.dumps({"taskId": seed.task_id})).task_id == seed.task_id
    with pytest.raises(ValidationError):
        GetTaskParams.model_validate_json(json.dumps({"task_id": seed.task_id}))
    assert TaskSubscriptionFilter(task_ids=[seed.task_id]).task_ids == [seed.task_id]
    with pytest.raises(ValueError):
        TaskSubscriptionFilter.model_validate_json(json.dumps({"task_ids": [seed.task_id]}))
