"""Protocol tests mirroring the Rust `mcp-task-extension/tests/protocol.rs`.

The Rust suite is the reference; these fixtures must not drift from it.
"""

import json
import uuid
from datetime import datetime, timezone
from typing import Any

import pytest
import uuid_extensions

from veoveo_mcp.task_extension import (
    EXTENSION_ID,
    PROTOCOL_VERSION,
    TASK_RETENTION_PIN_META_KEY,
    UPDATE_TASK_METHOD,
    AcknowledgeTaskResult,
    AdapterError,
    CancelTaskParams,
    CreateTaskResult,
    GetTaskParams,
    GetTaskResult,
    Implementation,
    RequestMeta,
    ServerDiscovery,
    Task,
    TaskExtensionMiddleware,
    TaskStatus,
    TaskSubscription,
    ToolCallParams,
    UpdateTaskParams,
    WorkingTask,
    dump,
)


def uuid7() -> uuid.UUID:
    return uuid_extensions.uuid7()


class FakeHandler:
    def __init__(self, task_id: uuid.UUID) -> None:
        self.task_id = task_id
        self.authentications = 0

    def task(self) -> Task:
        now = datetime.now(timezone.utc)
        return Task(
            task_id=self.task_id,
            status=TaskStatus.WORKING,
            status_message="working",
            created_at=now,
            last_updated_at=now,
            ttl_ms=60_000,
            poll_interval_ms=3_000,
        )

    def detailed(self) -> WorkingTask:
        task = self.task()
        return WorkingTask(
            task_id=task.task_id,
            status_message=task.status_message,
            created_at=task.created_at,
            last_updated_at=task.last_updated_at,
            ttl_ms=task.ttl_ms,
            poll_interval_ms=task.poll_interval_ms,
        )

    def authenticate(self, scope: dict[str, Any]) -> None:
        self.authentications += 1
        return None

    async def start_tool_task(
        self, caller: Any, request: ToolCallParams
    ) -> CreateTaskResult | None:
        if request.name != "forecast":
            return None
        return CreateTaskResult.from_task(self.task())

    async def get_task(self, caller: Any, request: GetTaskParams) -> GetTaskResult:
        return GetTaskResult(task=self.detailed())

    async def update_task(
        self, caller: Any, request: UpdateTaskParams
    ) -> AcknowledgeTaskResult:
        return AcknowledgeTaskResult()

    async def cancel_task(
        self, caller: Any, request: CancelTaskParams
    ) -> AcknowledgeTaskResult:
        return AcknowledgeTaskResult()

    async def subscribe_tasks(self, caller: Any, task_ids: list) -> TaskSubscription:
        async def updates():
            yield self.detailed()

        return TaskSubscription(accepted_task_ids=list(task_ids), updates=updates())


async def fallback_app(scope, receive, send):
    body = json.dumps({"forwarded": True}).encode()
    await send(
        {
            "type": "http.response.start",
            "status": 200,
            "headers": [(b"content-type", b"application/json")],
        }
    )
    await send({"type": "http.response.body", "body": body})


def app(handler: FakeHandler) -> TaskExtensionMiddleware:
    discovery = ServerDiscovery(
        capabilities={"tools": {}, "resources": {}},
        server_info=Implementation(name="test-server", version="1.0.0"),
    )
    return TaskExtensionMiddleware(fallback_app, handler, discovery)


def meta(with_tasks: bool) -> dict[str, Any]:
    extensions = {EXTENSION_ID: {}} if with_tasks else {}
    return {
        "io.modelcontextprotocol/protocolVersion": PROTOCOL_VERSION,
        "io.modelcontextprotocol/clientCapabilities": {"extensions": extensions},
    }


class Response:
    def __init__(self) -> None:
        self.status: int | None = None
        self.headers: dict[str, str] = {}
        self.body = b""

    async def send(self, message: dict[str, Any]) -> None:
        if message["type"] == "http.response.start":
            self.status = message["status"]
            self.headers = {
                name.decode(): value.decode() for name, value in message["headers"]
            }
        elif message["type"] == "http.response.body":
            self.body += message.get("body", b"")

    def json(self) -> Any:
        return json.loads(self.body)


async def request(
    middleware: TaskExtensionMiddleware,
    method: str,
    name: str | None,
    params: Any,
) -> Response:
    headers = [
        (b"content-type", b"application/json"),
        (b"mcp-protocol-version", PROTOCOL_VERSION.encode()),
        (b"mcp-method", method.encode()),
    ]
    if name is not None:
        headers.append((b"mcp-name", name.encode()))
    scope = {"type": "http", "method": "POST", "path": "/mcp", "headers": headers}
    body = json.dumps(
        {"jsonrpc": "2.0", "id": "request-1", "method": method, "params": params}
    ).encode()
    sent = False

    async def receive() -> dict[str, Any]:
        nonlocal sent
        if sent:
            return {"type": "http.disconnect"}
        sent = True
        return {"type": "http.request", "body": body, "more_body": False}

    response = Response()
    await middleware(scope, receive, response.send)
    return response


async def test_discovery_advertises_only_the_final_task_extension():
    handler = FakeHandler(uuid7())
    response = await request(
        app(handler), "server/discover", None, {"_meta": meta(False)}
    )
    assert response.status == 200
    body = response.json()
    assert body["result"]["supportedVersions"] == [PROTOCOL_VERSION]
    assert body["result"]["capabilities"]["extensions"][EXTENSION_ID] == {}
    assert handler.authentications == 1

    handler = FakeHandler(uuid7())
    response = await request(
        app(handler),
        "server/discover",
        None,
        {"_meta": {"io.modelcontextprotocol/protocolVersion": "2026-07-28"}},
    )
    assert response.status == 400
    assert response.json()["error"]["code"] == -32_602


async def test_task_creation_is_per_request_capability_gated():
    handler = FakeHandler(uuid7())
    response = await request(
        app(handler), "tools/call", "forecast", {"name": "forecast", "arguments": {}}
    )
    assert response.json()["forwarded"] is True
    assert handler.authentications == 0

    handler = FakeHandler(uuid7())
    middleware = app(handler)
    response = await request(
        middleware,
        "tools/call",
        "forecast",
        {"_meta": meta(False), "name": "forecast", "arguments": {}},
    )
    assert response.json()["forwarded"] is True

    response = await request(
        middleware,
        "tools/call",
        "forecast",
        {"_meta": meta(True), "name": "forecast", "arguments": {}},
    )
    body = response.json()
    assert body["result"]["resultType"] == "task"
    assert body["result"]["status"] == "working"

    handler = FakeHandler(uuid7())
    response = await request(
        app(handler),
        "tools/call",
        "forecast",
        {
            "_meta": meta(False),
            "name": "forecast",
            "arguments": {},
            "task": {"ttl": 60_000},
        },
    )
    assert response.status == 400
    assert response.json()["error"]["code"] == -32_602


async def test_lifecycle_methods_require_capability_and_exact_routing_headers():
    task_id = uuid7()
    handler = FakeHandler(task_id)
    response = await request(
        app(handler),
        "tasks/get",
        str(task_id),
        {"_meta": meta(False), "taskId": str(task_id)},
    )
    assert response.status == 400
    assert response.json()["error"]["code"] == -32_003

    response = await request(
        app(handler),
        "tasks/get",
        "wrong-task",
        {"_meta": meta(True), "taskId": str(task_id)},
    )
    assert response.status == 400
    assert response.json()["error"]["code"] == -32_602


async def test_update_and_cancel_have_final_shapes():
    task_id = uuid7()
    handler = FakeHandler(task_id)
    middleware = app(handler)
    for method in ["tasks/update", "tasks/cancel"]:
        if method == "tasks/update":
            params = {
                "_meta": meta(True),
                "taskId": str(task_id),
                "inputResponses": {},
            }
        else:
            params = {"_meta": meta(True), "taskId": str(task_id)}
        response = await request(middleware, method, str(task_id), params)
        assert response.json()["result"]["resultType"] == "complete"


async def test_subscription_stream_acknowledges_then_emits_full_task_notification():
    task_id = uuid7()
    handler = FakeHandler(task_id)
    response = await request(
        app(handler),
        "subscriptions/listen",
        None,
        {"_meta": meta(True), "notifications": {"taskIds": [str(task_id)]}},
    )
    assert response.status == 200
    assert response.headers["content-type"] == "text/event-stream"
    body = response.body.decode()
    assert "notifications/subscriptions/acknowledged" in body
    assert "notifications/tasks" in body
    assert "io.modelcontextprotocol/subscriptionId" in body


def test_constants_and_discriminators_match_final_sep():
    assert PROTOCOL_VERSION == "2026-06-30"
    assert EXTENSION_ID == "io.modelcontextprotocol/tasks"
    assert UPDATE_TASK_METHOD == "tasks/update"
    assert TASK_RETENTION_PIN_META_KEY == "ai.bioma.veoveo/taskRetentionPin"
    assert dump(AcknowledgeTaskResult()) == {"resultType": "complete"}


def test_request_ids_reject_non_v7_uuids():
    value = {
        "_meta": {
            "io.modelcontextprotocol/protocolVersion": PROTOCOL_VERSION,
            "io.modelcontextprotocol/clientCapabilities": {
                "extensions": {EXTENSION_ID: {}}
            },
        },
        "taskId": str(uuid.uuid4()),
    }
    with pytest.raises(Exception):
        GetTaskParams.model_validate(value)


def test_retention_pin_meta_is_typed_and_validated():
    parsed = RequestMeta.model_validate(
        {
            "io.modelcontextprotocol/protocolVersion": PROTOCOL_VERSION,
            "ai.bioma.veoveo/taskRetentionPin": "agent-episode:019",
        }
    )
    assert parsed.task_retention_pin == "agent-episode:019"
    with pytest.raises(Exception):
        RequestMeta.model_validate(
            {
                "io.modelcontextprotocol/protocolVersion": PROTOCOL_VERSION,
                "ai.bioma.veoveo/taskRetentionPin": "bad\nvalue",
            }
        )


def test_task_capability_requires_exactly_an_empty_object():
    strict_empty = RequestMeta.model_validate(
        {
            "io.modelcontextprotocol/protocolVersion": PROTOCOL_VERSION,
            "io.modelcontextprotocol/clientCapabilities": {
                "extensions": {EXTENSION_ID: {"unexpected": True}}
            },
        }
    )
    assert not strict_empty.declares_tasks()
