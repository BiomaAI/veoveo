"""Hosted MCP surface for an authoritative simulator-owned camera product."""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any

import mcp.types as types
from mcp.server import Server, ServerRequestContext
from mcp.server.caching import CacheHint
from mcp.shared.exceptions import MCPError
from pydantic import ValidationError

from veoveo_mcp.contract import (
    ContractDeclaration,
    DOC_ID_AGENTS,
    DOC_ID_DESIGN,
    ServerDocs,
    server_docs,
)
from veoveo_mcp.contract.identity import GatewayInternalIdentity
from veoveo_mcp.internal_auth import IDENTITY_SCOPE_KEY
from veoveo_mcp.schema import mcp_input_schema

from .contract import (
    CloseLiveViewRequest,
    CloseLiveViewResult,
    FixtureState,
    GetFixtureStateRequest,
    ListLiveCamerasRequest,
    LiveViewConnection,
    OpenLiveViewRequest,
    RenewLiveViewRequest,
)
from .runtime import FixtureRuntime


Context = ServerRequestContext[Any, Any]
APP_URI = "ui://anonymous-simulation/live.html"
APP_MIME = "text/html;profile=mcp-app"
INSTRUCTIONS = (
    "Authoritative simulation fixture. Its stable camera and NVIDIA H.264 product "
    "are simulator-owned; every actor and browser receives an independent ephemeral lease."
)
STATE_URI = "anonymous-simulation://state"
DOCS_URI = "anonymous-simulation://docs"
DESIGN_URI = "anonymous-simulation://docs/design"
AGENTS_URI = "anonymous-simulation://docs/agents"
CONTRACT_URI = "anonymous-simulation://contract"
SERVER_NAME = "anonymous-simulation"
_PACKAGE = __package__ or "anonymous_simulation_mcp"
_SOURCE_ROOT = Path(__file__).resolve().parents[2]
SERVER_DOCS: ServerDocs = server_docs(SERVER_NAME, _PACKAGE, source_root=_SOURCE_ROOT)
DOCS_INDEX = tuple(doc.wire() for doc in SERVER_DOCS)
LLMS_TXT = SERVER_DOCS.llms_txt()
AGENTS_DOCUMENT = SERVER_DOCS.doc(DOC_ID_AGENTS)
DESIGN_DOCUMENT = SERVER_DOCS.doc(DOC_ID_DESIGN)
if AGENTS_DOCUMENT is None or DESIGN_DOCUMENT is None:
    raise RuntimeError("shared server_docs omitted a required document")
CONTRACT_DECLARATION = ContractDeclaration.from_docs(SERVER_DOCS)


def build_mcp_server(runtime: FixtureRuntime) -> Server:
    def scope(ctx: Context) -> dict[str, Any]:
        request = ctx.request
        if request is None:
            raise _invalid("authenticated HTTP context missing")
        return request.scope

    async def list_tools(
        _ctx: Context, _params: types.PaginatedRequestParams | None
    ) -> types.ListToolsResult:
        app_meta = {"ui": {"resourceUri": APP_URI, "visibility": ["model", "app"]}}
        return types.ListToolsResult(
            tools=[
                types.Tool(
                    name="list_live_cameras",
                    title="List authoritative live cameras",
                    description="List stable logical cameras owned by this simulator.",
                    input_schema=mcp_input_schema(ListLiveCamerasRequest),
                    output_schema={"type": "array", "items": {}},
                    annotations=_annotations(read_only=True),
                    meta=app_meta,
                ),
                types.Tool(
                    name="open_live_view",
                    title="Open authoritative live view",
                    description="Reserve one viewer slot for this actor and browser instance.",
                    input_schema=mcp_input_schema(OpenLiveViewRequest),
                    output_schema=LiveViewConnection.model_json_schema(),
                    annotations=_annotations(),
                    meta=app_meta,
                ),
                types.Tool(
                    name="renew_live_view",
                    title="Renew authoritative live view",
                    description="Rotate only this viewer lease token and expiry.",
                    input_schema=mcp_input_schema(RenewLiveViewRequest),
                    output_schema=LiveViewConnection.model_json_schema(),
                    annotations=_annotations(),
                    meta=app_meta,
                ),
                types.Tool(
                    name="close_live_view",
                    title="Close authoritative live view",
                    description="Close only this actor and browser instance's lease.",
                    input_schema=mcp_input_schema(CloseLiveViewRequest),
                    output_schema=CloseLiveViewResult.model_json_schema(),
                    annotations=_annotations(destructive=True),
                    meta=app_meta,
                ),
                types.Tool(
                    name="get_fixture_state",
                    title="Read authoritative fixture state",
                    description="Read redacted camera, product, and aggregate viewer state.",
                    input_schema=mcp_input_schema(GetFixtureStateRequest),
                    output_schema=FixtureState.model_json_schema(),
                    annotations=_annotations(read_only=True),
                ),
            ]
        )

    async def call_tool(ctx: Context, params: types.CallToolRequestParams) -> types.CallToolResult:
        arguments = params.arguments or {}
        try:
            identity = _identity(scope(ctx))
            _require_operator_scope(identity)
            actor = identity.actor.id
            owner = _owner(identity)
            if params.name == "list_live_cameras":
                request = ListLiveCamerasRequest.model_validate(arguments)
                output = await runtime.list_live_cameras(request.session_id)
                return _structured("authoritative live cameras", output)
            if params.name == "open_live_view":
                output = await runtime.open(
                    actor, owner, OpenLiveViewRequest.model_validate(arguments)
                )
                return _structured("opened authoritative live view", output)
            if params.name == "renew_live_view":
                output = await runtime.renew(
                    actor, owner, RenewLiveViewRequest.model_validate(arguments)
                )
                return _structured("renewed authoritative live view", output)
            if params.name == "close_live_view":
                output = await runtime.close(
                    actor, owner, CloseLiveViewRequest.model_validate(arguments)
                )
                return _structured("closed authoritative live view", output)
            if params.name == "get_fixture_state":
                GetFixtureStateRequest.model_validate(arguments)
                return _structured("authoritative fixture state", await runtime.fixture_state())
        except MCPError as error:
            return _error_result(error.message)
        except (ValidationError, ValueError) as error:
            return _error_result(str(error))
        return _error_result(f"unknown tool `{params.name}`")

    async def list_resources(
        _ctx: Context, _params: types.PaginatedRequestParams | None
    ) -> types.ListResourcesResult:
        return types.ListResourcesResult(
            resources=[
                types.Resource(
                    uri=STATE_URI,
                    name="state",
                    title="Authoritative simulation fixture state",
                    description="Redacted camera, product, and viewer aggregates.",
                    mime_type="application/json",
                ),
                types.Resource(
                    uri=APP_URI,
                    name="live-app",
                    title="Authoritative live cameras",
                    description="Viewer for simulator-owned shared camera products.",
                    mime_type=APP_MIME,
                    meta={"ui": {"prefersBorder": True}},
                ),
                *[
                    types.Resource(
                        uri=uri,
                        name=name,
                        title=title,
                        description=description,
                        mime_type=mime,
                    )
                    for uri, name, title, description, mime in (
                        (DOCS_URI, "docs", "Fixture documentation", "Embedded document index.", "application/json"),
                        (DESIGN_URI, "design", "Fixture design", "Protocol and ownership boundary.", "text/markdown"),
                        (AGENTS_URI, "agents", "Fixture agent manual", "Implementation invariants.", "text/markdown"),
                        (CONTRACT_URI, "contract", "Fixture contract", "Machine-readable capability inventory.", "application/json"),
                    )
                ],
            ]
        )

    async def read_resource(ctx: Context, params: types.ReadResourceRequestParams) -> types.ReadResourceResult:
        uri = params.uri
        _identity(scope(ctx))
        if uri == STATE_URI:
            return _json_result(uri, (await runtime.fixture_state()).model_dump(mode="json", by_alias=True))
        if uri == APP_URI:
            return _text_result(uri, _APP_HTML, APP_MIME)
        if uri == DOCS_URI:
            return _json_result(uri, list(DOCS_INDEX))
        if uri == DESIGN_URI:
            return _text_result(uri, DESIGN_DOCUMENT.body, "text/markdown")
        if uri == AGENTS_URI:
            return _text_result(uri, AGENTS_DOCUMENT.body, "text/markdown")
        if uri == CONTRACT_URI:
            return _json_result(uri, CONTRACT_DECLARATION.wire())
        raise _invalid(f"unknown resource URI `{uri}`")

    return Server(
        SERVER_NAME,
        version="0.1.0",
        instructions=INSTRUCTIONS,
        cache_hints={
            "server/discover": CacheHint(ttl_ms=5_000, scope="private"),
            "tools/list": CacheHint(ttl_ms=5_000, scope="private"),
            "resources/list": CacheHint(ttl_ms=5_000, scope="private"),
            "resources/read": CacheHint(ttl_ms=1_000, scope="private"),
        },
        on_list_tools=list_tools,
        on_call_tool=call_tool,
        on_list_resources=list_resources,
        on_read_resource=read_resource,
        on_ping=None,
    )


def _identity(scope: dict[str, Any]) -> GatewayInternalIdentity:
    identity = scope.get(IDENTITY_SCOPE_KEY)
    if not isinstance(identity, GatewayInternalIdentity):
        raise _invalid("gateway identity missing")
    return identity


def _require_operator_scope(identity: GatewayInternalIdentity) -> None:
    if "operator:use" not in identity.actor.scopes:
        raise _invalid("operator:use scope is required")


def _owner(identity: GatewayInternalIdentity) -> str:
    subject = identity.authority.output_policy.owner
    return f"{subject.kind}:{subject.id}"


def _annotations(*, read_only: bool = False, destructive: bool = False) -> types.ToolAnnotations:
    return types.ToolAnnotations(
        read_only_hint=read_only,
        destructive_hint=destructive,
        idempotent_hint=read_only,
        open_world_hint=False,
    )


def _structured(text: str, output: Any) -> types.CallToolResult:
    if isinstance(output, tuple):
        structured = [item.model_dump(mode="json", by_alias=True, exclude_none=True) for item in output]
    else:
        structured = output.model_dump(mode="json", by_alias=True, exclude_none=True)
    return types.CallToolResult(
        content=[types.TextContent(type="text", text=text)],
        structured_content=structured,
        is_error=False,
    )


def _error_result(message: str) -> types.CallToolResult:
    return types.CallToolResult(
        content=[types.TextContent(type="text", text=message)], is_error=True
    )


def _json_result(uri: str, value: object) -> types.ReadResourceResult:
    return _text_result(uri, json.dumps(value, separators=(",", ":")), "application/json")


def _text_result(uri: str, body: str, mime: str) -> types.ReadResourceResult:
    return types.ReadResourceResult(
        contents=[types.TextResourceContents(uri=uri, text=body, mime_type=mime)]
    )


def _invalid(message: str) -> MCPError:
    return MCPError(code=types.INVALID_REQUEST, message=message)


_APP_HTML = """<!doctype html><html><head><meta charset=\"utf-8\"><title>Live cameras</title></head>
<body><main><h1>Authoritative live cameras</h1><p id=\"status\">Select a camera through the host.</p></main></body></html>"""
