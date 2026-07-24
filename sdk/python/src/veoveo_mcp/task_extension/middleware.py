"""ASGI middleware for the final MCP task extension.

Python equivalent of the Rust crate's `adapter.rs`: it intercepts extension
JSON-RPC methods and task-augmented `tools/call` requests before the inner MCP
application, validates the routing headers and per-request client capability,
and answers `subscriptions/listen` with an SSE notification stream.
"""

from __future__ import annotations

import json
from collections.abc import AsyncIterator, Awaitable, Callable, Sequence
from dataclasses import dataclass, field
from typing import Any, Protocol

from pydantic import BaseModel, ValidationError

from .models import (
    CANCEL_TASK_METHOD,
    DISCOVER_METHOD,
    EXTENSION_ID,
    GET_TASK_METHOD,
    HEADER_MCP_METHOD,
    HEADER_MCP_NAME,
    HEADER_MCP_PROTOCOL_VERSION,
    LISTEN_METHOD,
    MISSING_REQUIRED_CLIENT_CAPABILITY,
    PROTOCOL_VERSION,
    PROTOCOL_VERSION_META_KEY,
    SUBSCRIPTION_ACKNOWLEDGED_METHOD,
    SUBSCRIPTION_ID_META_KEY,
    TASK_NOTIFICATION_METHOD,
    UPDATE_TASK_METHOD,
    AcknowledgeTaskResult,
    CancelTaskParams,
    CreateTaskResult,
    DiscoverParams,
    DiscoverResult,
    GetTaskParams,
    GetTaskResult,
    Implementation,
    ListenParams,
    RequestMeta,
    ToolCallParams,
    UpdateTaskParams,
    dump,
)

MAX_REQUEST_BYTES = 2 * 1024 * 1024
JSON_RPC_VERSION = "2.0"

Scope = dict[str, Any]
Receive = Callable[[], Awaitable[dict[str, Any]]]
Send = Callable[[dict[str, Any]], Awaitable[None]]
AsgiApp = Callable[[Scope, Receive, Send], Awaitable[None]]

_HANDLED_METHODS = frozenset(
    [
        DISCOVER_METHOD,
        GET_TASK_METHOD,
        UPDATE_TASK_METHOD,
        CANCEL_TASK_METHOD,
        LISTEN_METHOD,
    ]
)


class AdapterError(Exception):
    def __init__(
        self,
        code: int,
        message: str,
        data: Any | None = None,
        http_status: int = 500,
    ) -> None:
        super().__init__(message)
        self.code = code
        self.message = message
        self.data = data
        self.http_status = http_status

    @classmethod
    def invalid_params(cls, message: str) -> "AdapterError":
        return cls(-32_602, message, http_status=400)

    @classmethod
    def unauthorized(cls, message: str) -> "AdapterError":
        return cls(-32_600, message, http_status=401)

    @classmethod
    def internal(cls, message: str) -> "AdapterError":
        return cls(-32_603, message, http_status=500)

    @classmethod
    def missing_task_capability(cls) -> "AdapterError":
        return cls(
            MISSING_REQUIRED_CLIENT_CAPABILITY,
            "missing required client capability",
            data={"requiredCapabilities": {"extensions": {EXTENSION_ID: {}}}},
            http_status=400,
        )


@dataclass
class TaskSubscription:
    accepted_task_ids: list[Any]
    updates: AsyncIterator[BaseModel]


class TaskExtensionHandler(Protocol):
    def authenticate(self, scope: Scope) -> Any: ...

    async def start_tool_task(
        self, caller: Any, request: ToolCallParams
    ) -> CreateTaskResult | None: ...

    async def get_task(self, caller: Any, request: GetTaskParams) -> GetTaskResult: ...

    async def update_task(
        self, caller: Any, request: UpdateTaskParams
    ) -> AcknowledgeTaskResult: ...

    async def cancel_task(
        self, caller: Any, request: CancelTaskParams
    ) -> AcknowledgeTaskResult: ...

    async def subscribe_tasks(
        self, caller: Any, task_ids: Sequence[Any]
    ) -> TaskSubscription: ...


@dataclass
class ServerDiscovery:
    capabilities: dict[str, Any]
    server_info: Implementation
    instructions: str | None = None
    _prepared: dict[str, Any] = field(init=False, default_factory=dict)

    def __post_init__(self) -> None:
        capabilities = dict(self.capabilities)
        extensions = capabilities.get("extensions")
        if not isinstance(extensions, dict):
            extensions = {}
        extensions = dict(extensions)
        extensions[EXTENSION_ID] = {}
        capabilities["extensions"] = extensions
        self._prepared = capabilities

    def result(self) -> DiscoverResult:
        return DiscoverResult(
            capabilities=self._prepared,
            server_info=self.server_info,
            instructions=self.instructions,
        )


class TaskExtensionMiddleware:
    def __init__(
        self, app: AsgiApp, handler: TaskExtensionHandler, discovery: ServerDiscovery
    ) -> None:
        self.app = app
        self.handler = handler
        self.discovery = discovery

    async def __call__(self, scope: Scope, receive: Receive, send: Send) -> None:
        if scope["type"] != "http" or scope.get("method") != "POST":
            await self.app(scope, receive, send)
            return

        try:
            body = await _read_body(receive)
        except AdapterError as error:
            await _send_error(send, None, error)
            return

        try:
            rpc = json.loads(body)
            if not isinstance(rpc, dict):
                raise ValueError("not an object")
            rpc_id = rpc["id"]
            method = rpc["method"]
            jsonrpc = rpc["jsonrpc"]
            if not isinstance(method, str) or not isinstance(jsonrpc, str):
                raise ValueError("invalid request fields")
        except (ValueError, KeyError):
            await self.app(scope, _replay(body, receive), send)
            return
        params = rpc.get("params")

        if jsonrpc != JSON_RPC_VERSION:
            await _send_error(
                send, rpc_id, AdapterError.invalid_params("jsonrpc must be `2.0`")
            )
            return

        handled = method in _HANDLED_METHODS
        extension_request = _request_protocol_version(params) is not None
        if not handled and not extension_request:
            await self.app(scope, _replay(body, receive), send)
            return

        headers = _headers(scope)
        try:
            caller = self.handler.authenticate(scope)
        except AdapterError as error:
            await _send_error(send, rpc_id, error)
            return

        try:
            _validate_protocol(headers, method, params)
        except AdapterError as error:
            await _send_error(send, rpc_id, error)
            return

        try:
            if method == DISCOVER_METHOD:
                _parse(DiscoverParams, params)
                await _send_result(send, rpc_id, dump(self.discovery.result()))
            elif method == GET_TASK_METHOD:
                request = _parse(GetTaskParams, params)
                _require_task_capability(request.meta)
                _validate_named_routing(headers, method, str(request.task_id))
                result = await self.handler.get_task(caller, request)
                await _send_result(send, rpc_id, result.wire())
            elif method == UPDATE_TASK_METHOD:
                request = _parse(UpdateTaskParams, params)
                _require_task_capability(request.meta)
                _validate_named_routing(headers, method, str(request.task_id))
                result = await self.handler.update_task(caller, request)
                await _send_result(send, rpc_id, dump(result))
            elif method == CANCEL_TASK_METHOD:
                request = _parse(CancelTaskParams, params)
                _require_task_capability(request.meta)
                _validate_named_routing(headers, method, str(request.task_id))
                result = await self.handler.cancel_task(caller, request)
                await _send_result(send, rpc_id, dump(result))
            elif method == LISTEN_METHOD:
                request = _parse(ListenParams, params)
                _require_task_capability(request.meta)
                if request.notifications.task_ids is None:
                    raise AdapterError.invalid_params(
                        "subscriptions/listen requires notifications.taskIds"
                    )
                subscription = await self.handler.subscribe_tasks(
                    caller, request.notifications.task_ids
                )
                await _send_subscription(send, rpc_id, subscription)
            elif method == "tools/call":
                request = _parse(ToolCallParams, params)
                _validate_named_routing(headers, "tools/call", request.name)
                if request.meta.declares_tasks():
                    created = await self.handler.start_tool_task(caller, request)
                    if created is not None:
                        await _send_result(send, rpc_id, dump(created))
                        return
                await self.app(
                    _without_protocol_header(scope),
                    _replay(json.dumps(rpc).encode(), receive),
                    send,
                )
            else:
                await self.app(
                    _without_protocol_header(scope), _replay(body, receive), send
                )
        except AdapterError as error:
            await _send_error(send, rpc_id, error)


async def _read_body(receive: Receive) -> bytes:
    chunks: list[bytes] = []
    total = 0
    while True:
        message = await receive()
        if message["type"] != "http.request":
            continue
        chunk = message.get("body", b"")
        total += len(chunk)
        if total > MAX_REQUEST_BYTES:
            raise AdapterError.invalid_params("reading request body failed: too large")
        chunks.append(chunk)
        if not message.get("more_body", False):
            return b"".join(chunks)


def _replay(body: bytes, upstream: Receive) -> Receive:
    sent = False

    async def receive() -> dict[str, Any]:
        nonlocal sent
        if not sent:
            sent = True
            return {"type": "http.request", "body": body, "more_body": False}
        return await upstream()

    return receive


def _headers(scope: Scope) -> dict[str, str]:
    headers: dict[str, str] = {}
    for name, value in scope.get("headers", []):
        headers.setdefault(name.decode("latin-1").lower(), value.decode("latin-1"))
    return headers


def _without_protocol_header(scope: Scope) -> Scope:
    stripped = dict(scope)
    stripped["headers"] = [
        (name, value)
        for name, value in scope.get("headers", [])
        if name.decode("latin-1").lower() != HEADER_MCP_PROTOCOL_VERSION
    ]
    return stripped


def _request_protocol_version(params: Any) -> str | None:
    if not isinstance(params, dict):
        return None
    meta = params.get("_meta")
    if not isinstance(meta, dict):
        return None
    version = meta.get(PROTOCOL_VERSION_META_KEY)
    return version if isinstance(version, str) else None


def _parse[T: BaseModel](model: type[T], params: Any) -> T:
    try:
        return model.model_validate(params if params is not None else {})
    except ValidationError as error:
        raise AdapterError.invalid_params(str(error)) from error


def _header_value(headers: dict[str, str], name: str) -> str:
    value = headers.get(name)
    if value is None:
        raise AdapterError.invalid_params(f"missing {name} header")
    return value


def _validate_protocol(headers: dict[str, str], method: str, params: Any) -> None:
    if _header_value(headers, HEADER_MCP_PROTOCOL_VERSION) != PROTOCOL_VERSION:
        raise AdapterError.invalid_params(
            f"MCP-Protocol-Version must be `{PROTOCOL_VERSION}`"
        )
    if _request_protocol_version(params) != PROTOCOL_VERSION:
        raise AdapterError.invalid_params(
            f"request _meta protocol version must be `{PROTOCOL_VERSION}`"
        )
    if _header_value(headers, HEADER_MCP_METHOD) != method:
        raise AdapterError.invalid_params(
            "Mcp-Method header does not match JSON-RPC method"
        )


def _validate_named_routing(headers: dict[str, str], method: str, name: str) -> None:
    if _header_value(headers, HEADER_MCP_METHOD) != method:
        raise AdapterError.invalid_params(
            "Mcp-Method header does not match JSON-RPC method"
        )
    if _header_value(headers, HEADER_MCP_NAME) != name:
        raise AdapterError.invalid_params(
            "Mcp-Name header does not match the routed task or tool"
        )


def _require_task_capability(meta: RequestMeta) -> None:
    if not meta.declares_tasks():
        raise AdapterError.missing_task_capability()


async def _send_json(
    send: Send, status: int, payload: dict[str, Any], content_type: bytes
) -> None:
    body = json.dumps(payload).encode()
    await send(
        {
            "type": "http.response.start",
            "status": status,
            "headers": [
                (b"content-type", content_type),
                (b"content-length", str(len(body)).encode()),
            ],
        }
    )
    await send({"type": "http.response.body", "body": body})


async def _send_result(send: Send, rpc_id: Any, result: dict[str, Any]) -> None:
    await _send_json(
        send,
        200,
        {"jsonrpc": JSON_RPC_VERSION, "id": rpc_id, "result": result},
        b"application/json",
    )


async def _send_error(send: Send, rpc_id: Any, error: AdapterError) -> None:
    await _send_json(
        send,
        error.http_status,
        {
            "jsonrpc": JSON_RPC_VERSION,
            "id": rpc_id,
            "error": {
                "code": error.code,
                "message": error.message,
                "data": error.data,
            },
        },
        b"application/json",
    )


def _sse_message(value: dict[str, Any]) -> bytes:
    return f"event: message\ndata: {json.dumps(value)}\n\n".encode()


async def _send_subscription(
    send: Send, rpc_id: Any, subscription: TaskSubscription
) -> None:
    await send(
        {
            "type": "http.response.start",
            "status": 200,
            "headers": [
                (b"content-type", b"text/event-stream"),
                (b"cache-control", b"no-cache"),
            ],
        }
    )
    acknowledged = {
        "jsonrpc": JSON_RPC_VERSION,
        "method": SUBSCRIPTION_ACKNOWLEDGED_METHOD,
        "params": {
            "_meta": {SUBSCRIPTION_ID_META_KEY: rpc_id},
            "notifications": {
                "taskIds": [str(task_id) for task_id in subscription.accepted_task_ids]
            },
        },
    }
    await send(
        {
            "type": "http.response.body",
            "body": _sse_message(acknowledged),
            "more_body": True,
        }
    )
    try:
        async for task in subscription.updates:
            params = dump(task)
            params["_meta"] = {SUBSCRIPTION_ID_META_KEY: rpc_id}
            notification = {
                "jsonrpc": JSON_RPC_VERSION,
                "method": TASK_NOTIFICATION_METHOD,
                "params": params,
            }
            await send(
                {
                    "type": "http.response.body",
                    "body": _sse_message(notification),
                    "more_body": True,
                }
            )
    except AdapterError:
        pass
    await send({"type": "http.response.body", "body": b"", "more_body": False})
