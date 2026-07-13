"""Platform task snapshot to protocol task projection.

Python port of the Rust `mcp-task-extension` projection module: the durable
runtime is the source of truth, the extension is transport only.
"""

from __future__ import annotations

from ..tasks.runtime import TaskRuntime
from ..tasks.types import InvalidRecord, TaskSnapshot, TaskStatus as StoreTaskStatus
from .models import (
    CancelledTask,
    CompletedTask,
    EmbeddedRequest,
    FailedTask,
    InputRequiredTask,
    JsonRpcErrorData,
    Task,
    TaskStatus,
    WorkingTask,
)


def task_seed(snapshot: TaskSnapshot) -> Task:
    return Task(
        task_id=snapshot.task_id,
        status=_task_status(snapshot.status),
        status_message=snapshot.status_message,
        created_at=snapshot.created_at,
        last_updated_at=snapshot.updated_at,
        ttl_ms=snapshot.ttl_ms,
        poll_interval_ms=snapshot.poll_interval_ms,
    )


async def project_snapshot(
    runtime: TaskRuntime, snapshot: TaskSnapshot
) -> WorkingTask | InputRequiredTask | CompletedTask | FailedTask | CancelledTask:
    metadata = {
        "task_id": snapshot.task_id,
        "status_message": snapshot.status_message,
        "created_at": snapshot.created_at,
        "last_updated_at": snapshot.updated_at,
        "ttl_ms": snapshot.ttl_ms,
        "poll_interval_ms": snapshot.poll_interval_ms,
    }
    status = snapshot.status
    if status in (
        StoreTaskStatus.QUEUED,
        StoreTaskStatus.RUNNING,
        StoreTaskStatus.CANCEL_REQUESTED,
    ):
        return WorkingTask(**metadata)
    if status == StoreTaskStatus.WAITING:
        requests = await runtime.outstanding_inputs(str(snapshot.task_id))
        if not requests:
            return WorkingTask(**metadata)
        return InputRequiredTask(
            **metadata,
            input_requests={
                key: EmbeddedRequest(method=request.method, params=request.params)
                for key, request in requests.items()
            },
        )
    if status == StoreTaskStatus.SUCCEEDED:
        if snapshot.result is None:
            raise InvalidRecord("completed task has no durable result")
        result = (
            snapshot.result
            if isinstance(snapshot.result, dict)
            else {"value": snapshot.result}
        )
        return CompletedTask(**metadata, result=result)
    if status == StoreTaskStatus.FAILED:
        failure = snapshot.error
        if failure is None:
            raise InvalidRecord("failed task has no durable error")
        return FailedTask(
            **metadata,
            error=JsonRpcErrorData(
                code=-32_603,
                message=failure.message,
                data={"taskCode": failure.code, "details": failure.details},
            ),
        )
    return CancelledTask(**metadata)


def _task_status(status: StoreTaskStatus) -> TaskStatus:
    if status in (
        StoreTaskStatus.QUEUED,
        StoreTaskStatus.RUNNING,
        StoreTaskStatus.WAITING,
        StoreTaskStatus.CANCEL_REQUESTED,
    ):
        return TaskStatus.WORKING
    if status == StoreTaskStatus.SUCCEEDED:
        return TaskStatus.COMPLETED
    if status == StoreTaskStatus.FAILED:
        return TaskStatus.FAILED
    return TaskStatus.CANCELLED
