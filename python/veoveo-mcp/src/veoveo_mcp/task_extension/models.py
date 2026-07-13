"""Wire models for the final MCP task extension, protocol version 2026-06-30.

This module is the Python equivalent of the Rust `veoveo-mcp-task-extension`
crate's `models.rs`. The constants and JSON shapes here are pinned to the same
final SEP revision; the Rust protocol tests are the reference fixtures.
"""

from __future__ import annotations

import uuid
from datetime import datetime
from enum import Enum
from typing import Annotated, Any, Literal, Union

from pydantic import (
    AfterValidator,
    BaseModel,
    ConfigDict,
    Field,
    PlainSerializer,
)

PROTOCOL_VERSION = "2026-06-30"
EXTENSION_ID = "io.modelcontextprotocol/tasks"
DISCOVER_METHOD = "server/discover"
GET_TASK_METHOD = "tasks/get"
UPDATE_TASK_METHOD = "tasks/update"
CANCEL_TASK_METHOD = "tasks/cancel"
LISTEN_METHOD = "subscriptions/listen"
TASK_NOTIFICATION_METHOD = "notifications/tasks"
SUBSCRIPTION_ACKNOWLEDGED_METHOD = "notifications/subscriptions/acknowledged"
CLIENT_CAPABILITIES_META_KEY = "io.modelcontextprotocol/clientCapabilities"
PROTOCOL_VERSION_META_KEY = "io.modelcontextprotocol/protocolVersion"
SUBSCRIPTION_ID_META_KEY = "io.modelcontextprotocol/subscriptionId"
TASK_RETENTION_PIN_META_KEY = "ai.bioma.veoveo/taskRetentionPin"
MISSING_REQUIRED_CLIENT_CAPABILITY = -32_003
HEADER_MCP_METHOD = "mcp-method"
HEADER_MCP_NAME = "mcp-name"
HEADER_MCP_PROTOCOL_VERSION = "mcp-protocol-version"


def _require_uuid_v7(value: uuid.UUID) -> uuid.UUID:
    if value.version != 7:
        raise ValueError("task id must be a UUIDv7")
    return value


ProtocolTaskId = Annotated[
    uuid.UUID,
    AfterValidator(_require_uuid_v7),
    PlainSerializer(str, return_type=str, when_used="always"),
]


def validate_retention_pin(value: str) -> str:
    if not value or len(value) > 256 or any(ch for ch in value if ch < " " or ch == "\x7f"):
        raise ValueError(
            "task retention pin is empty, too long, or contains a control character"
        )
    return value


TaskRetentionPin = Annotated[str, AfterValidator(validate_retention_pin)]


class TaskStatus(str, Enum):
    WORKING = "working"
    INPUT_REQUIRED = "input_required"
    COMPLETED = "completed"
    CANCELLED = "cancelled"
    FAILED = "failed"


class ClientCapabilities(BaseModel):
    model_config = ConfigDict(extra="allow")

    extensions: dict[str, Any] = Field(default_factory=dict)

    def declares_tasks(self) -> bool:
        declared = self.extensions.get(EXTENSION_ID)
        return isinstance(declared, dict) and not declared


class RequestMeta(BaseModel):
    model_config = ConfigDict(populate_by_name=True, extra="allow")

    protocol_version: str = Field(
        default=PROTOCOL_VERSION, alias=PROTOCOL_VERSION_META_KEY
    )
    client_capabilities: ClientCapabilities | None = Field(
        default=None, alias=CLIENT_CAPABILITIES_META_KEY
    )
    task_retention_pin: TaskRetentionPin | None = Field(
        default=None, alias=TASK_RETENTION_PIN_META_KEY
    )

    def with_task_capability(self) -> "RequestMeta":
        update = self.model_copy()
        update.client_capabilities = ClientCapabilities(extensions={EXTENSION_ID: {}})
        return update

    def declares_tasks(self) -> bool:
        return (
            self.client_capabilities is not None
            and self.client_capabilities.declares_tasks()
        )


class _CamelModel(BaseModel):
    model_config = ConfigDict(populate_by_name=True)


class Task(_CamelModel):
    task_id: ProtocolTaskId = Field(alias="taskId")
    status: TaskStatus
    status_message: str | None = Field(default=None, alias="statusMessage")
    created_at: datetime = Field(alias="createdAt")
    last_updated_at: datetime = Field(alias="lastUpdatedAt")
    ttl_ms: int | None = Field(default=None, alias="ttlMs")
    poll_interval_ms: int | None = Field(default=None, alias="pollIntervalMs")


class _TaskMetadataFields(_CamelModel):
    task_id: ProtocolTaskId = Field(alias="taskId")
    status_message: str | None = Field(default=None, alias="statusMessage")
    created_at: datetime = Field(alias="createdAt")
    last_updated_at: datetime = Field(alias="lastUpdatedAt")
    ttl_ms: int | None = Field(default=None, alias="ttlMs")
    poll_interval_ms: int | None = Field(default=None, alias="pollIntervalMs")


class EmbeddedRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")

    method: str
    params: dict[str, Any] = Field(default_factory=dict)


class JsonRpcErrorData(BaseModel):
    code: int
    message: str
    data: Any | None = None


class WorkingTask(_TaskMetadataFields):
    status: Literal["working"] = "working"


class InputRequiredTask(_TaskMetadataFields):
    status: Literal["input_required"] = "input_required"
    input_requests: dict[str, EmbeddedRequest] = Field(alias="inputRequests")


class CompletedTask(_TaskMetadataFields):
    status: Literal["completed"] = "completed"
    result: dict[str, Any]


class FailedTask(_TaskMetadataFields):
    status: Literal["failed"] = "failed"
    error: JsonRpcErrorData


class CancelledTask(_TaskMetadataFields):
    status: Literal["cancelled"] = "cancelled"


DetailedTask = Annotated[
    Union[WorkingTask, InputRequiredTask, CompletedTask, FailedTask, CancelledTask],
    Field(discriminator="status"),
]


class CreateTaskResult(Task):
    result_type: Literal["task"] = Field(default="task", alias="resultType")

    @classmethod
    def from_task(cls, task: Task) -> "CreateTaskResult":
        return cls.model_validate({"resultType": "task", **dump(task)})


class GetTaskResult(BaseModel):
    model_config = ConfigDict(populate_by_name=True)

    result_type: Literal["complete"] = Field(default="complete", alias="resultType")
    task: DetailedTask

    def wire(self) -> dict[str, Any]:
        return {"resultType": "complete", **dump(self.task)}

    @classmethod
    def from_wire(cls, value: dict[str, Any]) -> "GetTaskResult":
        body = dict(value)
        result_type = body.pop("resultType", None)
        if result_type != "complete":
            raise ValueError("resultType must be `complete`")
        return cls(task=body)


class AcknowledgeTaskResult(BaseModel):
    model_config = ConfigDict(populate_by_name=True)

    result_type: Literal["complete"] = Field(default="complete", alias="resultType")


class GetTaskParams(_CamelModel):
    model_config = ConfigDict(populate_by_name=True, extra="forbid")

    meta: RequestMeta = Field(alias="_meta")
    task_id: ProtocolTaskId = Field(alias="taskId")


class UpdateTaskParams(_CamelModel):
    model_config = ConfigDict(populate_by_name=True, extra="forbid")

    meta: RequestMeta = Field(alias="_meta")
    task_id: ProtocolTaskId = Field(alias="taskId")
    input_responses: dict[str, dict[str, Any]] = Field(alias="inputResponses")


class CancelTaskParams(_CamelModel):
    model_config = ConfigDict(populate_by_name=True, extra="forbid")

    meta: RequestMeta = Field(alias="_meta")
    task_id: ProtocolTaskId = Field(alias="taskId")


class ToolCallParams(_CamelModel):
    model_config = ConfigDict(populate_by_name=True, extra="forbid")

    meta: RequestMeta = Field(alias="_meta")
    name: str
    arguments: dict[str, Any] = Field(default_factory=dict)


class DiscoverParams(BaseModel):
    model_config = ConfigDict(populate_by_name=True, extra="forbid")

    meta: RequestMeta = Field(alias="_meta")


class Implementation(BaseModel):
    name: str
    version: str


class DiscoverResult(_CamelModel):
    result_type: Literal["complete"] = Field(default="complete", alias="resultType")
    supported_versions: list[str] = Field(
        default_factory=lambda: [PROTOCOL_VERSION], alias="supportedVersions"
    )
    capabilities: dict[str, Any]
    server_info: Implementation = Field(alias="serverInfo")
    instructions: str | None = None


class NotificationSelection(_CamelModel):
    model_config = ConfigDict(populate_by_name=True, extra="forbid")

    task_ids: list[ProtocolTaskId] | None = Field(default=None, alias="taskIds")


class ListenParams(BaseModel):
    model_config = ConfigDict(populate_by_name=True, extra="forbid")

    meta: RequestMeta = Field(alias="_meta")
    notifications: NotificationSelection


def dump(model: BaseModel) -> dict[str, Any]:
    """Serialize a wire model exactly as the Rust contract does."""
    return model.model_dump(mode="json", by_alias=True, exclude_none=True)
