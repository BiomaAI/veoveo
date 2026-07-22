"""The datasheet MCP surface: tools, resources, templates, prompts, completions.

Built on the low-level `mcp` SDK server so pagination, typed structured
content, and completions match the Rust servers' behavior.
"""

from __future__ import annotations

import asyncio
import json
from typing import Any

import mcp.types as types
from mcp.server.lowlevel import Server
from mcp.server.lowlevel.helper_types import ReadResourceContents
from mcp.shared.exceptions import McpError

from veoveo_mcp.contract import UsageKind, UsageRecord, UsageReport
from veoveo_mcp.pagination import PaginationError, paginate
from veoveo_mcp.schema import mcp_input_schema
from veoveo_mcp.tasks import parse_task_id

from .. import engine, prompts, uris
from ..contract import (
    ColumnStatsOutput,
    ColumnStatsRequest,
    DatasetSelector,
    PreviewDatasetOutput,
    PreviewDatasetRequest,
    ProfileDatasetOutput,
    ProfileDatasetRequest,
)
from .app_state import AppState
from .ownership import (
    caller_from_scope,
    identity_from_scope,
    request_scope,
    require_task_owner,
    runtime_owner,
    task_owner_allows,
)
from .profile_task import SERVER_SLUG

LIST_PAGE_SIZE = 100
INSTRUCTIONS = (
    "Datasheet profiling server. Use direct tools for small previews and "
    "column statistics; run profile_dataset as an MCP task for the full "
    "profile and shared-plane artifact output. Resources expose reports, "
    "per-task usage, and artifacts under the datasheet:// scheme."
)


def _invalid(message: str) -> McpError:
    return McpError(types.ErrorData(code=types.INVALID_REQUEST, message=message))


def _internal(message: str) -> McpError:
    return McpError(types.ErrorData(code=types.INTERNAL_ERROR, message=message))


def build_mcp_server(state: AppState) -> Server:
    server: Server = Server("datasheet", version="0.1.0", instructions=INSTRUCTIONS)

    def scope() -> dict[str, Any]:
        return request_scope(server)

    async def load_frame(selector: DatasetSelector):
        if selector.inline_csv is not None:
            return await asyncio.to_thread(engine.load_inline_csv, selector.inline_csv)
        caller = caller_from_scope(scope())
        artifact = await state.artifacts.resolve(caller, selector.dataset_uri or "")
        return await asyncio.to_thread(
            engine.load_dataframe,
            artifact.bytes_,
            artifact.metadata.filename,
            artifact.metadata.mime_type,
        )

    @server.list_tools()
    async def list_tools() -> list[types.Tool]:
        return [
            types.Tool(
                name="preview_dataset",
                title="Preview dataset",
                description=(
                    "Read the schema and a small sample of a CSV or Parquet "
                    "dataset from an artifact URI or inline CSV."
                ),
                inputSchema=mcp_input_schema(PreviewDatasetRequest),
                outputSchema=PreviewDatasetOutput.model_json_schema(),
                annotations=types.ToolAnnotations(
                    readOnlyHint=True,
                    destructiveHint=False,
                    idempotentHint=True,
                    openWorldHint=False,
                ),
            ),
            types.Tool(
                name="column_stats",
                title="Column statistics",
                description="Compute summary statistics for one dataset column.",
                inputSchema=mcp_input_schema(ColumnStatsRequest),
                outputSchema=ColumnStatsOutput.model_json_schema(),
                annotations=types.ToolAnnotations(
                    readOnlyHint=True,
                    destructiveHint=False,
                    idempotentHint=True,
                    openWorldHint=False,
                ),
            ),
            types.Tool(
                name="profile_dataset",
                title="Profile dataset",
                description=(
                    "Run a full dataset profile as an MCP task and optionally "
                    "store the JSON report through the shared artifact plane."
                ),
                inputSchema=mcp_input_schema(ProfileDatasetRequest),
                outputSchema=ProfileDatasetOutput.model_json_schema(),
                annotations=types.ToolAnnotations(
                    readOnlyHint=False,
                    destructiveHint=False,
                    idempotentHint=False,
                    openWorldHint=False,
                ),
            ),
        ]

    @server.call_tool()
    async def call_tool(name: str, arguments: dict[str, Any]) -> types.CallToolResult:
        if name == "preview_dataset":
            request = PreviewDatasetRequest.model_validate(arguments)
            try:
                frame = await load_frame(request)
                output = engine.preview(frame, request.rows)
            except engine.EngineError as error:
                raise _invalid(str(error)) from error
            return _structured_result(
                f"previewed {len(output.rows)} of {output.row_count} row(s)", output
            )
        if name == "column_stats":
            request = ColumnStatsRequest.model_validate(arguments)
            try:
                frame = await load_frame(request)
                output = engine.column_stats(frame, request.column)
            except engine.EngineError as error:
                raise _invalid(str(error)) from error
            return _structured_result(f"column {output.column} statistics", output)
        if name == "profile_dataset":
            raise _invalid("profile_dataset requires task-based invocation")
        raise _invalid(f"unknown tool `{name}`")

    @server.list_resources()
    async def list_resources(
        request: types.ListResourcesRequest,
    ) -> types.ListResourcesResult:
        identity = identity_from_scope(scope())
        resources = [
            types.Resource(
                uri=uris.REPORTS_URI,
                name="reports",
                title="Profile reports",
                description="Completed and running datasheet profile tasks.",
                mimeType="application/json",
            ),
            types.Resource(
                uri=uris.USAGE_ROOT_URI,
                name="usage",
                title="Datasheet usage ledger",
                description="Index of task usage resources.",
                mimeType="application/json",
            ),
        ]
        for task_id in await state.tasks.store.domain_usage_task_ids(SERVER_SLUG):
            owner = await state.tasks.owner(str(task_id))
            if owner is None or not task_owner_allows(owner, identity):
                continue
            resources.append(
                types.Resource(
                    uri=uris.usage_task_uri(str(task_id)),
                    name=f"usage for task {task_id}",
                    description="Usage rows for one datasheet task.",
                    mimeType="application/json",
                )
            )
        resources.sort(key=lambda resource: str(resource.uri))
        cursor = request.params.cursor if request.params is not None else None
        try:
            page = paginate(resources, cursor, LIST_PAGE_SIZE)
        except PaginationError as error:
            raise _invalid(str(error)) from error
        return types.ListResourcesResult(
            resources=page.items, nextCursor=page.next_cursor
        )

    @server.list_resource_templates()
    async def list_resource_templates() -> list[types.ResourceTemplate]:
        return [
            types.ResourceTemplate(
                uriTemplate=uris.USAGE_TASK_TEMPLATE,
                name="usage",
                title="Datasheet task usage",
                description=(
                    "Usage rows for one datasheet task. task_id supports "
                    "completion."
                ),
                mimeType="application/json",
            ),
            types.ResourceTemplate(
                uriTemplate=uris.ARTIFACT_TEMPLATE,
                name="artifact",
                title="Datasheet artifact",
                description="Shared-plane immutable datasheet artifact.",
                mimeType="application/json",
            ),
        ]

    @server.read_resource()
    async def read_resource(uri: Any) -> list[ReadResourceContents]:
        text = str(uri)
        identity = identity_from_scope(scope())
        if text == uris.REPORTS_URI:
            snapshots = await state.tasks.list_for_owner(runtime_owner(identity))
            reports = [
                {
                    "task_id": str(snapshot.task_id),
                    "task_type": snapshot.task_type,
                    "status": snapshot.status.value,
                    "usage_uri": uris.usage_task_uri(str(snapshot.task_id)),
                    "created_at": snapshot.created_at.isoformat(),
                }
                for snapshot in snapshots
            ]
            return [_json_contents(reports)]
        if text == uris.USAGE_ROOT_URI:
            entries = []
            for task_id in await state.tasks.store.domain_usage_task_ids(SERVER_SLUG):
                owner = await state.tasks.owner(str(task_id))
                if owner is not None and task_owner_allows(owner, identity):
                    entries.append(
                        {
                            "task_id": str(task_id),
                            "usage_uri": uris.usage_task_uri(str(task_id)),
                        }
                    )
            return [_json_contents(entries)]
        task_id = uris.parse_usage_task_uri(text)
        if task_id is not None:
            await require_task_owner(state, identity, task_id)
            records = await state.tasks.store.domain_usage_for_task(
                SERVER_SLUG, parse_task_id(task_id)
            )
            if not records:
                raise _invalid(f"unknown usage task `{task_id}`")
            report = UsageReport.build(
                task_id,
                uris.usage_task_uri(task_id),
                [_usage_record(task_id, record) for record in records],
            )
            return [_json_contents(report.wire())]
        artifact_id = uris.parse_artifact_uri(text)
        if artifact_id is not None:
            caller = caller_from_scope(scope())
            artifact = await state.artifacts.get(caller, artifact_id)
            if artifact is None:
                raise _invalid(f"unknown artifact `{artifact_id}`")
            return [
                ReadResourceContents(
                    content=artifact.bytes_,
                    mime_type=artifact.metadata.mime_type or "application/json",
                )
            ]
        raise _invalid(f"unknown resource uri: {text}")

    @server.completion()
    async def complete(
        ref: types.PromptReference | types.ResourceTemplateReference,
        argument: types.CompletionArgument,
        _context: types.CompletionContext | None,
    ) -> types.Completion | None:
        if (
            isinstance(ref, types.ResourceTemplateReference)
            and ref.uri == uris.USAGE_TASK_TEMPLATE
            and argument.name == "task_id"
        ):
            identity = identity_from_scope(scope())
            values = []
            for task_id in await state.tasks.store.domain_usage_task_ids(SERVER_SLUG):
                owner = await state.tasks.owner(str(task_id))
                if owner is None or not task_owner_allows(owner, identity):
                    continue
                if str(task_id).startswith(argument.value):
                    values.append(str(task_id))
            total = len(values)
            values = values[:100]
            return types.Completion(
                values=values, total=total, hasMore=len(values) < total
            )
        return None

    @server.list_prompts()
    async def list_prompts() -> list[types.Prompt]:
        return prompts.list_prompts()

    @server.get_prompt()
    async def get_prompt(
        name: str, arguments: dict[str, str] | None
    ) -> types.GetPromptResult:
        try:
            return prompts.get_prompt(name, arguments)
        except ValueError as error:
            raise _invalid(str(error)) from error

    return server


def _structured_result(text: str, output: Any) -> types.CallToolResult:
    return types.CallToolResult(
        content=[types.TextContent(type="text", text=text)],
        structuredContent=output.model_dump(mode="json", exclude_none=True),
        isError=False,
    )


def _json_contents(value: Any) -> ReadResourceContents:
    return ReadResourceContents(
        content=json.dumps(value), mime_type="application/json"
    )


def _usage_record(task_id: str, record: dict[str, Any]) -> UsageRecord:
    metadata = record.get("metadata") or {}
    return UsageRecord(
        task_id=task_id,
        source_id=record.get("source_id"),
        provider_job_id=record.get("provider_job_id"),
        model_id=record["model_id"],
        kind=UsageKind(record["kind"]),
        quantity=record.get("quantity"),
        unit=record.get("unit"),
        amount=record.get("amount"),
        currency=record.get("currency"),
        recorded_at=record["recorded_at"],
        metadata=metadata,
    )
