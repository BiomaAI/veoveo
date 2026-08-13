"""Typed bindings for the MCP 2026-07-28 Tasks extension (SEP-2663).

The Python SDK 2.0 lifecycle owns discovery, request metadata, routing headers,
result stamping, and Streamable HTTP. This module contains only the task types
that are not yet shipped by that SDK release.
"""

from __future__ import annotations

from datetime import datetime
from typing import Annotated, Any, Literal, Union

import mcp.types as types
from pydantic import AfterValidator, BaseModel, ConfigDict, Field

PROTOCOL_VERSION = "2026-07-28"
EXTENSION_ID = "io.modelcontextprotocol/tasks"
GET_TASK_METHOD = "tasks/get"
UPDATE_TASK_METHOD = "tasks/update"
CANCEL_TASK_METHOD = "tasks/cancel"
TASK_NOTIFICATION_METHOD = "notifications/tasks"
SUBSCRIPTION_ID_META_KEY = "io.modelcontextprotocol/subscriptionId"
TASK_RETENTION_PIN_META_KEY = "ai.bioma.veoveo/taskRetentionPin"


def _validate_task_id(value: str) -> str:
    if not value or len(value.encode()) > 1024:
        raise ValueError("task id must contain 1..1024 encoded bytes")
    if any(ch < " " or ch == "\x7f" for ch in value):
        raise ValueError("task id must not contain control characters")
    return value


OpaqueTaskId = Annotated[str, AfterValidator(_validate_task_id)]


def validate_retention_pin(value: str) -> str:
    if not value or len(value) > 256 or any(ch < " " or ch == "\x7f" for ch in value):
        raise ValueError(
            "task retention pin is empty, too long, or contains a control character"
        )
    return value


TaskRetentionPin = Annotated[str, AfterValidator(validate_retention_pin)]


class TaskStatus(str):
    WORKING = "working"
    INPUT_REQUIRED = "input_required"
    COMPLETED = "completed"
    CANCELLED = "cancelled"
    FAILED = "failed"


TaskStatusValue = Literal[
    "working", "input_required", "completed", "cancelled", "failed"
]


def _to_camel(value: str) -> str:
    first, *rest = value.split("_")
    return first + "".join(part.capitalize() for part in rest)


class _TaskModel(BaseModel):
    model_config = ConfigDict(alias_generator=_to_camel, populate_by_name=True)


class Task(_TaskModel):
    task_id: OpaqueTaskId
    status: TaskStatusValue
    status_message: str | None = None
    created_at: datetime
    last_updated_at: datetime
    ttl_ms: int | None = Field(default=None, ge=0)
    poll_interval_ms: int | None = Field(default=None, ge=0)


class _TaskMetadataFields(_TaskModel):
    task_id: OpaqueTaskId
    status_message: str | None = None
    created_at: datetime
    last_updated_at: datetime
    ttl_ms: int | None = Field(default=None, ge=0)
    poll_interval_ms: int | None = Field(default=None, ge=0)


class WorkingTask(_TaskMetadataFields):
    status: Literal["working"] = "working"


class InputRequiredTask(_TaskMetadataFields):
    status: Literal["input_required"] = "input_required"
    input_requests: types.InputRequests


class CompletedTask(_TaskMetadataFields):
    status: Literal["completed"] = "completed"
    result: dict[str, Any]


class FailedTask(_TaskMetadataFields):
    status: Literal["failed"] = "failed"
    error: dict[str, Any]


class CancelledTask(_TaskMetadataFields):
    status: Literal["cancelled"] = "cancelled"


DetailedTask = Annotated[
    Union[WorkingTask, InputRequiredTask, CompletedTask, FailedTask, CancelledTask],
    Field(discriminator="status"),
]


class CreateTaskResult(Task, types.Result):
    result_type: Literal["task"] = "task"

    @classmethod
    def from_task(cls, task: Task) -> "CreateTaskResult":
        return cls.model_validate({"resultType": "task", **dump(task)})


class GetTaskResult(types.Result):
    """Typed internal result whose ``wire`` method emits the flattened shape."""

    result_type: Literal["complete"] = "complete"
    task: DetailedTask

    def wire(self) -> dict[str, Any]:
        return {"resultType": "complete", **dump(self.task)}

    @classmethod
    def from_wire(cls, value: dict[str, Any]) -> "GetTaskResult":
        body = dict(value)
        if body.pop("resultType", None) != "complete":
            raise ValueError("resultType must be `complete`")
        return cls(task=body)


class AcknowledgeTaskResult(types.Result):
    result_type: Literal["complete"] = "complete"


class GetTaskParams(types.RequestParams):
    task_id: OpaqueTaskId


class UpdateTaskParams(types.RequestParams):
    task_id: OpaqueTaskId
    input_responses: types.InputResponses


class CancelTaskParams(types.RequestParams):
    task_id: OpaqueTaskId


class TaskSubscriptionFilter(types.SubscriptionFilter):
    task_ids: list[OpaqueTaskId] | None = None


class TaskSubscriptionsListenParams(types.RequestParams):
    notifications: TaskSubscriptionFilter


class TaskStatusNotificationParams(types.NotificationParams):
    task: DetailedTask

    def wire(self) -> dict[str, Any]:
        value = dump(self.task)
        if self.meta is not None:
            value["_meta"] = self.meta
        return value


class TaskStatusNotification(BaseModel):
    method: Literal["notifications/tasks"] = "notifications/tasks"
    params: TaskStatusNotificationParams

    def model_dump(self, **kwargs: Any) -> dict[str, Any]:
        return {"method": self.method, "params": self.params.wire()}


def dump(model: BaseModel) -> dict[str, Any]:
    return model.model_dump(mode="json", by_alias=True, exclude_none=True)
