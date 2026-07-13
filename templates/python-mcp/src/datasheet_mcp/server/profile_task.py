"""Durable profile_dataset task: submission, worker, artifact output, resume.

The dataset is materialized while the gateway identity is live and embedded
in the durable task request, so `resume` recovery re-runs the profile from
persisted state alone. Artifact output redeems a task-bound write capability
issued at submission; the background worker never mints an identity.
"""

from __future__ import annotations

import asyncio
import base64
import uuid
from datetime import datetime, timedelta, timezone
from typing import Any

from veoveo_mcp.contract import (
    IssueArtifactWriteCapabilityRequest,
    IssuedArtifactWriteCapability,
    PlaneCaller,
    PutArtifactRequest,
)
from veoveo_mcp.contract.identity import GatewayInternalIdentity
from veoveo_mcp.tasks import (
    Conflict,
    CreateTask,
    LeaseHeld,
    RecoveryClass,
    TaskSnapshot,
    TaskTransition,
    new_task_id,
)

from .. import engine
from ..contract import ProfileDatasetOutput, ProfileDatasetRequest
from .app_state import AppState, update_task
from .ownership import runtime_owner

SERVER_SLUG = "datasheet"
TASK_TYPE = "profile_dataset"
MODEL_ID = "datasheet/profile"
MCP_TASK_POLL_INTERVAL_MS = 1_000
MCP_TASK_TTL_MS = 7 * 24 * 60 * 60 * 1_000
TASK_LEASE = timedelta(seconds=120)
TASK_LEASE_HEARTBEAT = timedelta(seconds=40)
ARTIFACT_CAPABILITY_TTL = timedelta(hours=24)
ARTIFACT_MIME = "application/json"


class ProfileTaskError(Exception):
    pass


async def materialize_dataset(
    state: AppState, caller: PlaneCaller, request: ProfileDatasetRequest
) -> tuple[bytes, str | None, str | None]:
    if request.inline_csv is not None:
        data = request.inline_csv.encode()
        name, mime = "inline.csv", "text/csv"
    else:
        artifact = await state.artifacts.resolve(caller, request.dataset_uri or "")
        data = artifact.bytes_
        name = artifact.metadata.filename
        mime = artifact.metadata.mime_type
    if len(data) > state.max_dataset_bytes:
        raise ProfileTaskError(
            f"dataset is {len(data)} bytes; the durable request limit is "
            f"{state.max_dataset_bytes}"
        )
    return data, name, mime


async def start_profile_task(
    state: AppState,
    identity: GatewayInternalIdentity,
    caller: PlaneCaller,
    args: ProfileDatasetRequest,
    retention_pins: frozenset[str],
) -> TaskSnapshot:
    task_id = new_task_id()
    data, dataset_name, dataset_mime = await materialize_dataset(state, caller, args)
    capability: IssuedArtifactWriteCapability | None = None
    if args.artifact:
        capability = await state.artifacts.issue_write_capability(
            caller,
            IssueArtifactWriteCapabilityRequest(
                task_id=str(task_id),
                expires_at=datetime.now(timezone.utc) + ARTIFACT_CAPABILITY_TTL,
                max_artifact_count=1,
                max_total_bytes=state.max_artifact_bytes,
            ),
        )
    request_payload: dict[str, Any] = {
        "args": args.model_dump(mode="json"),
        "dataset_b64": base64.b64encode(data).decode(),
        "dataset_name": dataset_name,
        "dataset_mime": dataset_mime,
        "artifact_write_capability": (
            capability.model_dump(mode="json") if capability is not None else None
        ),
    }
    created = await state.tasks.create(
        CreateTask(
            task_id=task_id,
            owner=runtime_owner(identity),
            server=SERVER_SLUG,
            task_type=TASK_TYPE,
            request=request_payload,
            recovery_class=RecoveryClass.RESUME,
            idempotency_key=None,
            ttl_ms=MCP_TASK_TTL_MS,
            poll_interval_ms=MCP_TASK_POLL_INTERVAL_MS,
            retention_pins=retention_pins,
        )
    )
    return await schedule_profile_task(state, created.snapshot)


async def schedule_profile_task(state: AppState, snapshot: TaskSnapshot) -> TaskSnapshot:
    task_id = str(snapshot.task_id)
    claimed = await state.tasks.claim(task_id, TASK_LEASE)
    cancellation = asyncio.Event()
    worker = asyncio.create_task(_run_task(state, task_id, snapshot.request, cancellation))
    state.tasks.register_worker(task_id, cancellation, worker)
    return claimed.snapshot


async def resume_profile_tasks(state: AppState) -> int:
    recovery = await state.tasks.recover()
    resumed = 0
    for snapshot in recovery.resumable:
        if snapshot.task_type != TASK_TYPE:
            continue
        try:
            await schedule_profile_task(state, snapshot)
            resumed += 1
        except (LeaseHeld, Conflict):
            state.logger.info(
                "another replica claimed recovered datasheet task",
                task_id=str(snapshot.task_id),
            )
    return resumed


async def _run_task(
    state: AppState,
    task_id: str,
    request_payload: dict[str, Any],
    cancellation: asyncio.Event,
) -> None:
    work = asyncio.create_task(
        _run_task_inner(state, task_id, request_payload, cancellation)
    )
    while True:
        done, _ = await asyncio.wait(
            [work], timeout=TASK_LEASE_HEARTBEAT.total_seconds()
        )
        if done:
            break
        try:
            await state.tasks.renew_lease(task_id, TASK_LEASE)
        except Exception as error:  # noqa: BLE001 — lease loss cancels the work
            state.logger.warn(
                f"task lease heartbeat failed: {error}", task_id=task_id
            )
            cancellation.set()
            break
    await work


async def _complete_tool_error(state: AppState, task_id: str, message: str) -> None:
    result = {
        "content": [{"type": "text", "text": message}],
        "isError": True,
    }
    await update_task(state, task_id, TaskTransition.succeeded(message, result))


async def _run_task_inner(
    state: AppState,
    task_id: str,
    request_payload: dict[str, Any],
    cancellation: asyncio.Event,
) -> None:
    async def fail(message: str) -> None:
        state.logger.warn(f"datasheet task failed: {message}", task_id=task_id)
        await _complete_tool_error(state, task_id, message)

    await update_task(
        state,
        task_id,
        TaskTransition.running("profiling dataset", 0.1),
    )
    try:
        args = ProfileDatasetRequest.model_validate(request_payload["args"])
        data = base64.b64decode(request_payload["dataset_b64"])
        dataset_name = request_payload.get("dataset_name")
        dataset_mime = request_payload.get("dataset_mime")
        raw_capability = request_payload.get("artifact_write_capability")
        capability = (
            IssuedArtifactWriteCapability.model_validate(raw_capability)
            if raw_capability is not None
            else None
        )
    except Exception as error:  # noqa: BLE001 — durable request must be typed
        await fail(f"invalid durable task request: {error}")
        return

    try:
        frame = await asyncio.to_thread(
            engine.load_dataframe, data, dataset_name, dataset_mime
        )
        profile = await asyncio.to_thread(engine.profile, frame, args.histogram_bins)
    except engine.EngineError as error:
        await fail(str(error))
        return
    if cancellation.is_set():
        await update_task(state, task_id, TaskTransition.cancelled())
        return

    output = ProfileDatasetOutput(profile=profile, artifact=None)
    if args.artifact:
        if capability is None:
            await fail("task did not reserve artifact write capability")
            return
        report_bytes = output.profile.model_dump_json(indent=2).encode()
        try:
            metadata = await state.artifacts.put_with_capability(
                capability,
                f"datasheet:{task_id}:profile",
                PutArtifactRequest(
                    mime_type=ARTIFACT_MIME,
                    filename=f"datasheet-profile-{task_id}.json",
                    metadata={
                        "task_id": task_id,
                        "artifact_format": "datasheet_profile_json",
                        "row_count": profile.row_count,
                        "column_count": profile.column_count,
                    },
                ),
                report_bytes,
            )
        except Exception as error:  # noqa: BLE001 — plane errors are terminal here
            await fail(f"artifact write failed: {error}")
            return
        output.artifact = metadata.without_download_url()

    try:
        await state.tasks.store.upsert_domain_usage(
            task_id=uuid.UUID(task_id),
            server=SERVER_SLUG,
            model_id=MODEL_ID,
            kind="actual",
            quantity=float(profile.column_count),
            unit="column",
            metadata={
                "row_count": profile.row_count,
                "column_count": profile.column_count,
            },
        )
    except Exception as error:  # noqa: BLE001
        await fail(f"usage record failed: {error}")
        return

    if cancellation.is_set():
        await update_task(state, task_id, TaskTransition.cancelled())
        return

    content: list[dict[str, Any]] = [
        {
            "type": "text",
            "text": (
                f"profiled {profile.row_count} row(s) across "
                f"{profile.column_count} column(s)"
            ),
        }
    ]
    if output.artifact is not None:
        content.append(
            {
                "type": "resource_link",
                "uri": output.artifact.artifact_uri,
                "name": "datasheet profile report",
                "title": "datasheet profile report",
                "description": "JSON artifact containing the full dataset profile.",
                "mimeType": ARTIFACT_MIME,
            }
        )
    result = {
        "content": content,
        "structuredContent": output.model_dump(mode="json", exclude_none=True),
        "isError": False,
    }
    await update_task(
        state,
        task_id,
        TaskTransition.succeeded("dataset profile completed", result),
    )
