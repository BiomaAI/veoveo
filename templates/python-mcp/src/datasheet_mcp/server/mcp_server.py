"""The datasheet MCP surface: tools, resources, templates, prompts, completions.

Built on the low-level `mcp` SDK server so pagination, typed structured
content, and completions match the Rust servers' behavior.
"""

from __future__ import annotations

import asyncio
import base64
import json
from typing import Any

import mcp.types as types
from mcp.server import Server, ServerRequestContext
from mcp.server.caching import CacheHint
from mcp.shared.exceptions import MCPError
from pydantic import ValidationError

from veoveo_mcp.contract import UsageKind, UsageRecord, UsageReport
from veoveo_mcp.pagination import PaginationError, paginate
from veoveo_mcp.schema import mcp_input_schema
from veoveo_mcp.tasks import parse_task_id

from .. import engine, prompts, uris
from ..docs import CONTRACT_DECLARATION, SERVER_DOCS
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

Context = ServerRequestContext[Any, Any]

LIST_PAGE_SIZE = 100
INSTRUCTIONS = (
    "Datasheet profiling server. Use direct tools for small previews and "
    "column statistics; run profile_dataset as an MCP task for the full "
    "profile and shared-plane artifact output. Resources expose reports, "
    "per-task usage, and artifacts under the datasheet:// scheme."
)


def _invalid(message: str) -> MCPError:
    return MCPError(code=types.INVALID_REQUEST, message=message)


def build_mcp_server(state: AppState) -> Server:
    async def load_frame(ctx: Context, selector: DatasetSelector):
        if selector.inline_csv is not None:
            return await asyncio.to_thread(engine.load_inline_csv, selector.inline_csv)
        caller = caller_from_scope(request_scope(ctx))
        artifact = await state.artifacts.resolve(caller, selector.dataset_uri or "")
        return await asyncio.to_thread(
            engine.load_dataframe,
            artifact.bytes_,
            artifact.metadata.filename,
            artifact.metadata.mime_type,
        )

    async def list_tools(
        _ctx: Context, _params: types.PaginatedRequestParams | None
    ) -> types.ListToolsResult:
        return types.ListToolsResult(
            tools=[
                types.Tool(
                    name="preview_dataset",
                    title="Preview dataset",
                    description=(
                        "Read the schema and a small sample of a CSV or Parquet "
                        "dataset from an artifact URI or inline CSV."
                    ),
                    input_schema=mcp_input_schema(PreviewDatasetRequest),
                    output_schema=PreviewDatasetOutput.model_json_schema(),
                    annotations=types.ToolAnnotations(
                        read_only_hint=True,
                        destructive_hint=False,
                        idempotent_hint=True,
                        open_world_hint=False,
                    ),
                ),
                types.Tool(
                    name="column_stats",
                    title="Column statistics",
                    description="Compute summary statistics for one dataset column.",
                    input_schema=mcp_input_schema(ColumnStatsRequest),
                    output_schema=ColumnStatsOutput.model_json_schema(),
                    annotations=types.ToolAnnotations(
                        read_only_hint=True,
                        destructive_hint=False,
                        idempotent_hint=True,
                        open_world_hint=False,
                    ),
                ),
                types.Tool(
                    name="profile_dataset",
                    title="Profile dataset",
                    description=(
                        "Run a full dataset profile as an MCP task and optionally "
                        "store the JSON report through the shared artifact plane."
                    ),
                    input_schema=mcp_input_schema(ProfileDatasetRequest),
                    output_schema=ProfileDatasetOutput.model_json_schema(),
                    annotations=types.ToolAnnotations(
                        read_only_hint=False,
                        destructive_hint=False,
                        idempotent_hint=False,
                        open_world_hint=False,
                    ),
                ),
            ]
        )

    async def call_tool(
        ctx: Context, params: types.CallToolRequestParams
    ) -> types.CallToolResult:
        name = params.name
        arguments = params.arguments or {}
        try:
            if name == "preview_dataset":
                request = PreviewDatasetRequest.model_validate(arguments)
                frame = await load_frame(ctx, request)
                output = engine.preview(frame, request.rows)
                return _structured_result(
                    f"previewed {len(output.rows)} of {output.row_count} row(s)",
                    output,
                )
            if name == "column_stats":
                request = ColumnStatsRequest.model_validate(arguments)
                frame = await load_frame(ctx, request)
                output = engine.column_stats(frame, request.column)
                return _structured_result(f"column {output.column} statistics", output)
            if name == "profile_dataset":
                return _error_result("profile_dataset requires task-based invocation")
            return _error_result(f"unknown tool `{name}`")
        except MCPError as error:
            return _error_result(error.message)
        except (engine.EngineError, ValidationError) as error:
            return _error_result(str(error))

    async def list_resources(
        ctx: Context, params: types.PaginatedRequestParams | None
    ) -> types.ListResourcesResult:
        identity = identity_from_scope(request_scope(ctx))
        resources = [
            types.Resource(
                uri=uris.REPORTS_URI,
                name="reports",
                title="Profile reports",
                description="Completed and running datasheet profile tasks.",
                mime_type="application/json",
            ),
            types.Resource(
                uri=uris.USAGE_ROOT_URI,
                name="usage",
                title="Datasheet usage ledger",
                description="Index of task usage resources.",
                mime_type="application/json",
            ),
            types.Resource(
                uri=uris.DOCS_URI,
                name="docs",
                title="Datasheet server documentation",
                description="Index of embedded server documents.",
                mime_type="application/json",
            ),
        ]
        for doc in SERVER_DOCS:
            resources.append(
                types.Resource(
                    uri=uris.doc_uri(doc.id),
                    name=doc.id,
                    title=doc.title,
                    description=f"Embedded `{doc.id}` server document.",
                    mime_type="text/markdown",
                )
            )
        resources.append(
            types.Resource(
                uri=uris.CONTRACT_URI,
                name="contract",
                title="Datasheet contract declaration",
                description=(
                    "Machine-readable contract revision, compliance, and "
                    "capability inventory."
                ),
                mime_type="application/json",
            )
        )
        for task_id in await state.tasks.store.domain_usage_task_ids(SERVER_SLUG):
            owner = await state.tasks.owner(str(task_id))
            if owner is None or not task_owner_allows(owner, identity):
                continue
            resources.append(
                types.Resource(
                    uri=uris.usage_task_uri(str(task_id)),
                    name=f"usage for task {task_id}",
                    description="Usage rows for one datasheet task.",
                    mime_type="application/json",
                )
            )
        resources.sort(key=lambda resource: resource.uri)
        cursor = params.cursor if params is not None else None
        try:
            page = paginate(resources, cursor, LIST_PAGE_SIZE)
        except PaginationError as error:
            raise _invalid(str(error)) from error
        return types.ListResourcesResult(
            resources=page.items, next_cursor=page.next_cursor
        )

    async def list_resource_templates(
        _ctx: Context, _params: types.PaginatedRequestParams | None
    ) -> types.ListResourceTemplatesResult:
        return types.ListResourceTemplatesResult(
            resource_templates=[
                types.ResourceTemplate(
                    uri_template=uris.USAGE_TASK_TEMPLATE,
                    name="usage",
                    title="Datasheet task usage",
                    description=(
                        "Usage rows for one datasheet task. task_id supports "
                        "completion."
                    ),
                    mime_type="application/json",
                ),
                types.ResourceTemplate(
                    uri_template=uris.ARTIFACT_TEMPLATE,
                    name="artifact",
                    title="Datasheet artifact",
                    description="Shared-plane immutable datasheet artifact.",
                    mime_type="application/json",
                ),
            ]
        )

    async def read_resource(
        ctx: Context, params: types.ReadResourceRequestParams
    ) -> types.ReadResourceResult:
        text = params.uri
        identity = identity_from_scope(request_scope(ctx))
        if text == uris.DOCS_URI:
            return _json_result(text, SERVER_DOCS.index_wire())
        if text == uris.CONTRACT_URI:
            return _json_result(text, CONTRACT_DECLARATION.wire())
        doc_id = uris.parse_doc_uri(text)
        if doc_id is not None:
            doc = SERVER_DOCS.doc(doc_id)
            if doc is None:
                raise _invalid(f"unknown server document `{doc_id}`")
            return types.ReadResourceResult(
                contents=[
                    types.TextResourceContents(
                        uri=text, text=doc.body, mime_type="text/markdown"
                    )
                ]
            )
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
            return _json_result(text, reports)
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
            return _json_result(text, entries)
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
            return _json_result(text, report.wire())
        artifact_id = uris.parse_artifact_uri(text)
        if artifact_id is not None:
            caller = caller_from_scope(request_scope(ctx))
            artifact = await state.artifacts.get(caller, artifact_id)
            if artifact is None:
                raise _invalid(f"unknown artifact `{artifact_id}`")
            return types.ReadResourceResult(
                contents=[
                    types.BlobResourceContents(
                        uri=text,
                        blob=base64.b64encode(artifact.bytes_).decode(),
                        mime_type=artifact.metadata.mime_type or "application/json",
                    )
                ]
            )
        raise _invalid(f"unknown resource uri: {text}")

    async def complete(
        ctx: Context, params: types.CompleteRequestParams
    ) -> types.CompleteResult:
        ref = params.ref
        argument = params.argument
        if (
            isinstance(ref, types.ResourceTemplateReference)
            and ref.uri == uris.USAGE_TASK_TEMPLATE
            and argument.name == "task_id"
        ):
            identity = identity_from_scope(request_scope(ctx))
            values = []
            for task_id in await state.tasks.store.domain_usage_task_ids(SERVER_SLUG):
                owner = await state.tasks.owner(str(task_id))
                if owner is None or not task_owner_allows(owner, identity):
                    continue
                if str(task_id).startswith(argument.value):
                    values.append(str(task_id))
            total = len(values)
            values = values[:100]
            return types.CompleteResult(
                completion=types.Completion(
                    values=values, total=total, has_more=len(values) < total
                )
            )
        return types.CompleteResult(
            completion=types.Completion(values=[], total=None, has_more=None)
        )

    async def list_prompts(
        _ctx: Context, _params: types.PaginatedRequestParams | None
    ) -> types.ListPromptsResult:
        return types.ListPromptsResult(prompts=prompts.list_prompts())

    async def get_prompt(
        _ctx: Context, params: types.GetPromptRequestParams
    ) -> types.GetPromptResult:
        try:
            return prompts.get_prompt(params.name, params.arguments)
        except ValueError as error:
            raise _invalid(str(error)) from error

    return Server(
        "datasheet",
        version="0.1.0",
        instructions=INSTRUCTIONS,
        cache_hints={
            "server/discover": CacheHint(ttl_ms=5_000, scope="private"),
            "tools/list": CacheHint(ttl_ms=5_000, scope="private"),
            "resources/list": CacheHint(ttl_ms=5_000, scope="private"),
            "resources/templates/list": CacheHint(ttl_ms=5_000, scope="private"),
            "resources/read": CacheHint(ttl_ms=1_000, scope="private"),
            "prompts/list": CacheHint(ttl_ms=5_000, scope="private"),
        },
        on_list_tools=list_tools,
        on_call_tool=call_tool,
        on_list_resources=list_resources,
        on_list_resource_templates=list_resource_templates,
        on_read_resource=read_resource,
        on_completion=complete,
        on_list_prompts=list_prompts,
        on_get_prompt=get_prompt,
        on_ping=None,
    )


def _structured_result(text: str, output: Any) -> types.CallToolResult:
    return types.CallToolResult(
        content=[types.TextContent(type="text", text=text)],
        structured_content=output.model_dump(mode="json", exclude_none=True),
        is_error=False,
    )


def _error_result(message: str) -> types.CallToolResult:
    return types.CallToolResult(
        content=[types.TextContent(type="text", text=message)],
        is_error=True,
    )


def _json_result(uri: str, value: Any) -> types.ReadResourceResult:
    return types.ReadResourceResult(
        contents=[
            types.TextResourceContents(
                uri=uri, text=json.dumps(value), mime_type="application/json"
            )
        ]
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
