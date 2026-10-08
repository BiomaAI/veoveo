"""Typed bindings for the MCP 2026-07-28 Tasks extension (SEP-2663).

The Python SDK 2.0 lifecycle owns discovery, request metadata, routing headers,
result stamping, and Streamable HTTP. This module contains only the task types
that are not yet shipped by that SDK release.
"""

from __future__ import annotations

from datetime import datetime
from typing import Annotated, Any, Literal, Self, Union

import mcp.types as types
from pydantic import (
    AfterValidator, BaseModel, ConfigDict, Field, TypeAdapter, ValidationInfo,
    field_validator, model_validator,
)

from ..contract.wire import CurrentWireModel

PROTOCOL_VERSION = "2026-07-28"
EXTENSION_ID = "io.modelcontextprotocol/tasks"
GET_TASK_METHOD = "tasks/get"
UPDATE_TASK_METHOD = "tasks/update"
CANCEL_TASK_METHOD = "tasks/cancel"
TASK_NOTIFICATION_METHOD = "notifications/tasks"
SUBSCRIPTION_ID_META_KEY = "io.modelcontextprotocol/subscriptionId"
TASK_RETENTION_PIN_META_KEY = "ai.veoveo/task-retention-pin"


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


class _WireAdmission:
    """External decoding uses aliases; typed constructor kwargs stay ergonomic."""

    @classmethod
    def model_validate(cls, value: Any, **kwargs: Any) -> Self:
        context = kwargs.get("context")
        kwargs["context"] = {
            **(context if isinstance(context, dict) else {}), "wire_admission": True,
        }
        kwargs.setdefault("by_alias", True)
        kwargs.setdefault("by_name", False)
        return super().model_validate(value, **kwargs)

    @classmethod
    def model_validate_json(cls, value: str | bytes | bytearray, **kwargs: Any) -> Self:
        context = kwargs.get("context")
        kwargs["context"] = {
            **(context if isinstance(context, dict) else {}), "wire_admission": True,
        }
        kwargs.setdefault("by_alias", True)
        kwargs.setdefault("by_name", False)
        return super().model_validate_json(value, **kwargs)


class _TaskModel(_WireAdmission, CurrentWireModel):
    model_config = ConfigDict(
        alias_generator=_to_camel, validate_by_name=True,
        extra="forbid", hide_input_in_errors=True,
    )

    @model_validator(mode="before")
    @classmethod
    def current_wire_keys(cls, value: object, info: ValidationInfo) -> object:
        if info.mode == "json" or (
            isinstance(info.context, dict) and info.context.get("wire_admission")
        ):
            return super().current_wire_keys(value)
        return value


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
        value = {"resultType": "complete", **dump(self.task)}
        if self.meta is not None:
            value["_meta"] = self.meta
        return value

    @classmethod
    def from_wire(cls, value: dict[str, Any]) -> "GetTaskResult":
        body = dict(value)
        if body.pop("resultType", None) != "complete":
            raise ValueError("resultType must be `complete`")
        meta = body.pop("_meta", None)
        task = TypeAdapter(DetailedTask).validate_python(
            body, by_alias=True, by_name=False, context={"wire_admission": True},
        )
        return cls(task=task, meta=meta)


class AcknowledgeTaskResult(types.Result):
    result_type: Literal["complete"] = "complete"


class _TaskParams(_TaskModel, types.RequestParams):
    model_config = ConfigDict(extra="forbid", hide_input_in_errors=True)


class GetTaskParams(_TaskParams):
    task_id: OpaqueTaskId


class UpdateTaskParams(_TaskParams):
    task_id: OpaqueTaskId
    input_responses: types.InputResponses


class CancelTaskParams(_TaskParams):
    task_id: OpaqueTaskId


class TaskSubscriptionFilter(_WireAdmission, types.SubscriptionFilter):
    # The upstream filter is open for independently contributed extensions.
    task_ids: list[OpaqueTaskId] | None = None

    @classmethod
    def model_validate(cls, value: Any, **kwargs: Any) -> Self:
        if isinstance(value, dict):
            for name, field in cls.model_fields.items():
                if field.alias != name and name in value:
                    raise ValueError("subscription filter contains a retired field name")
        return super().model_validate(value, **kwargs)

    @classmethod
    def model_validate_json(cls, value: str | bytes | bytearray, **kwargs: Any) -> Self:
        # Decode JSON through the same field admission without mirroring the model.
        from pydantic import JsonValue
        decoded = TypeAdapter(JsonValue).validate_json(value)
        return cls.model_validate(decoded, **kwargs)


class TaskSubscriptionsListenParams(_TaskParams):
    notifications: TaskSubscriptionFilter

    @field_validator("notifications", mode="before")
    @classmethod
    def admitted_notifications(cls, value: Any) -> TaskSubscriptionFilter:
        if isinstance(value, TaskSubscriptionFilter):
            return value
        return TaskSubscriptionFilter.model_validate(value)


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
