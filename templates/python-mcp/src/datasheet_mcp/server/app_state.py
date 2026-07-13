"""Dependency composition shared by the MCP surface and the task extension."""

from __future__ import annotations

from dataclasses import dataclass

from veoveo_mcp.artifacts import ArtifactRepository
from veoveo_mcp.tasks import TaskRuntime, TaskTransition
from veoveo_mcp.telemetry import JsonLogger


@dataclass
class AppState:
    tasks: TaskRuntime
    artifacts: ArtifactRepository
    logger: JsonLogger
    max_artifact_bytes: int
    max_dataset_bytes: int


async def update_task(state: AppState, task_id: str, transition: TaskTransition) -> None:
    try:
        await state.tasks.transition(task_id, transition)
    except Exception as error:  # noqa: BLE001 — best-effort status publication
        state.logger.warn(
            f"datasheet task update failed: {error}", task_id=task_id
        )
