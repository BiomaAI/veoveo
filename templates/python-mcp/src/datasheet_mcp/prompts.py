"""Reusable prompt workflows for the datasheet server."""

from __future__ import annotations

import mcp.types as types

from . import uris

PROFILE_PROMPT = "datasheet-profile-dataset"
REVIEW_PROMPT = "datasheet-report-review"


def list_prompts() -> list[types.Prompt]:
    return [
        types.Prompt(
            name=PROFILE_PROMPT,
            title="Profile a dataset",
            description=(
                "Guide a full dataset profile: preview the schema first, then "
                "run profile_dataset as an MCP task and read the report artifact."
            ),
            arguments=[
                types.PromptArgument(
                    name="dataset_uri",
                    description="Artifact URI of the CSV or Parquet dataset.",
                    required=True,
                ),
            ],
        ),
        types.Prompt(
            name=REVIEW_PROMPT,
            title="Review a profile report",
            description="Summarize one completed datasheet profile task.",
            arguments=[
                types.PromptArgument(
                    name="task_id",
                    description="Completed profile task id.",
                    required=True,
                ),
            ],
        ),
    ]


def get_prompt(name: str, arguments: dict[str, str] | None) -> types.GetPromptResult:
    arguments = arguments or {}
    if name == PROFILE_PROMPT:
        dataset_uri = arguments.get("dataset_uri", "<artifact uri>")
        text = (
            f"Profile the dataset at `{dataset_uri}`.\n\n"
            "1. Call `preview_dataset` with the dataset URI to inspect the "
            "schema and a small sample.\n"
            "2. Call `profile_dataset` as an MCP task (task-augmented "
            "tools/call) with `artifact: true` so the full report is stored "
            "on the shared artifact plane.\n"
            "3. Poll `tasks/get` until the task completes, then read the "
            "returned `datasheet://artifact/{id}` resource for the report and "
            "the matching `datasheet://usage/task/{id}` resource for usage."
        )
        return types.GetPromptResult(
            description="Dataset profiling workflow",
            messages=[
                types.PromptMessage(
                    role="user",
                    content=types.TextContent(type="text", text=text),
                )
            ],
        )
    if name == REVIEW_PROMPT:
        task_id = arguments.get("task_id", "<task id>")
        text = (
            f"Review datasheet profile task `{task_id}`.\n\n"
            f"Read `{uris.usage_task_uri(task_id)}` for recorded usage and the "
            "task result for the report artifact link. Summarize row/column "
            "counts, columns with a high null share, and the strongest "
            "correlations."
        )
        return types.GetPromptResult(
            description="Profile report review",
            messages=[
                types.PromptMessage(
                    role="user",
                    content=types.TextContent(type="text", text=text),
                )
            ],
        )
    raise ValueError(f"unknown prompt `{name}`")
