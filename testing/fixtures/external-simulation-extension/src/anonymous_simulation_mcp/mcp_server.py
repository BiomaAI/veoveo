"""Hosted MCP surface for assets and the synthetic pose producer."""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any

import mcp.types as types
from mcp.server import Server, ServerRequestContext
from mcp.shared.exceptions import MCPError
from pydantic import ValidationError

from veoveo_mcp.contract import (
    CapabilityInventory,
    ContractDeclaration,
    DOC_ID_AGENTS,
    DOC_ID_DESIGN,
    ServerDocs,
    server_docs,
)
from veoveo_mcp.contract.identity import GatewayInternalIdentity, PlaneCaller
from veoveo_mcp.internal_auth import BEARER_SCOPE_KEY, IDENTITY_SCOPE_KEY
from veoveo_mcp.schema import mcp_input_schema

from .contract import (
    FixtureState,
    GetFixtureStateRequest,
    PrepareSceneRequest,
    PreparedScene,
    ProducerState,
    StartPoseProducerRequest,
    StopPoseProducerRequest,
)
from .runtime import FixtureRuntime


Context = ServerRequestContext[Any, Any]

INSTRUCTIONS = (
    "Anonymous external Simulation View fixture. Publish its synthetic "
    "OpenUSD scene, then authorize and start its independent mTLS pose "
    "producer. Camera and live-view operations belong to Simulation View."
)
STATE_URI = "anonymous-simulation://state"
DOCS_URI = "anonymous-simulation://docs"
DESIGN_URI = "anonymous-simulation://docs/design"
AGENTS_URI = "anonymous-simulation://docs/agents"
CONTRACT_URI = "anonymous-simulation://contract"
SERVER_NAME = "anonymous-simulation"
_PACKAGE = __package__ or "anonymous_simulation_mcp"
_SOURCE_ROOT = Path(__file__).resolve().parents[2]
SERVER_DOCS: ServerDocs = server_docs(
    SERVER_NAME, _PACKAGE, source_root=_SOURCE_ROOT
)
DOCS_INDEX = tuple(doc.wire() for doc in SERVER_DOCS)
LLMS_TXT = SERVER_DOCS.llms_txt()
AGENTS_DOCUMENT = SERVER_DOCS.doc(DOC_ID_AGENTS)
DESIGN_DOCUMENT = SERVER_DOCS.doc(DOC_ID_DESIGN)
if AGENTS_DOCUMENT is None or DESIGN_DOCUMENT is None:
    raise RuntimeError("shared server_docs omitted a required document")
CAPABILITY_INVENTORY = CapabilityInventory(
    tools=(
        "prepare_scene",
        "start_pose_producer",
        "stop_pose_producer",
        "get_fixture_state",
    ),
    resources=(
        STATE_URI,
        DOCS_URI,
        AGENTS_URI,
        DESIGN_URI,
        CONTRACT_URI,
    ),
)
CONTRACT_DECLARATION = ContractDeclaration.from_docs(
    SERVER_DOCS, CAPABILITY_INVENTORY
)


def build_mcp_server(runtime: FixtureRuntime) -> Server:
    def scope(ctx: Context) -> dict[str, Any]:
        request = ctx.request
        if request is None:
            raise _invalid("authenticated HTTP context missing")
        return request.scope

    async def list_tools(
        _ctx: Context, _params: types.PaginatedRequestParams | None
    ) -> types.ListToolsResult:
        return types.ListToolsResult(
            tools=[
                types.Tool(
                    name="prepare_scene",
                    title="Prepare synthetic Simulation View scene",
                    description=(
                        "Publish fixture-owned OpenUSD assets to the Artifact plane "
                        "and return one immutable Simulation View scene declaration."
                    ),
                    input_schema=mcp_input_schema(PrepareSceneRequest),
                    output_schema=PreparedScene.model_json_schema(),
                    annotations=types.ToolAnnotations(
                        read_only_hint=False,
                        destructive_hint=False,
                        idempotent_hint=False,
                        open_world_hint=False,
                    ),
                ),
                types.Tool(
                    name="start_pose_producer",
                    title="Start synthetic pose producer",
                    description=(
                        "Start complete moving-entity snapshots on the independent "
                        "mTLS Simulation View pose data plane."
                    ),
                    input_schema=mcp_input_schema(StartPoseProducerRequest),
                    output_schema=ProducerState.model_json_schema(),
                    annotations=types.ToolAnnotations(
                        read_only_hint=False,
                        destructive_hint=False,
                        idempotent_hint=True,
                        open_world_hint=False,
                    ),
                ),
                types.Tool(
                    name="stop_pose_producer",
                    title="Stop synthetic pose producer",
                    description="Stop the fixture-owned pose stream.",
                    input_schema=mcp_input_schema(StopPoseProducerRequest),
                    output_schema=ProducerState.model_json_schema(),
                    annotations=types.ToolAnnotations(
                        read_only_hint=False,
                        destructive_hint=True,
                        idempotent_hint=True,
                        open_world_hint=False,
                    ),
                ),
                types.Tool(
                    name="get_fixture_state",
                    title="Read synthetic producer state",
                    description="Read redacted pose-producer lifecycle and counters.",
                    input_schema=mcp_input_schema(GetFixtureStateRequest),
                    output_schema=FixtureState.model_json_schema(),
                    annotations=types.ToolAnnotations(
                        read_only_hint=True,
                        destructive_hint=False,
                        idempotent_hint=True,
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
            if name == "prepare_scene":
                request = PrepareSceneRequest.model_validate(arguments)
                output = await runtime.prepare_scene(_caller(scope(ctx)), request)
                return _structured("prepared synthetic scene", output)
            if name == "start_pose_producer":
                request = StartPoseProducerRequest.model_validate(arguments)
                output = await runtime.start(request)
                return _structured("started synthetic pose producer", output)
            if name == "stop_pose_producer":
                request = StopPoseProducerRequest.model_validate(arguments)
                output = await runtime.stop(request.session_id)
                return _structured("stopped synthetic pose producer", output)
            if name == "get_fixture_state":
                GetFixtureStateRequest.model_validate(arguments)
                output = await runtime.fixture_state()
                return _structured("read synthetic fixture state", output)
        except MCPError as error:
            return _error_result(error.message)
        except (ValidationError, ValueError) as error:
            return _error_result(str(error))
        return _error_result(f"unknown tool `{name}`")

    async def list_resources(
        _ctx: Context, _params: types.PaginatedRequestParams | None
    ) -> types.ListResourcesResult:
        resources = [
            types.Resource(
                uri=STATE_URI,
                name="state",
                title="Anonymous simulation fixture state",
                description="Redacted synthetic pose-producer state.",
                mime_type="application/json",
            ),
            types.Resource(
                uri=DOCS_URI,
                name="docs",
                title="Anonymous simulation fixture documentation",
                description="Index of embedded fixture documents.",
                mime_type="application/json",
            ),
            types.Resource(
                uri=DESIGN_URI,
                name="design",
                title="Anonymous simulation fixture design",
                description="Public protocol and ownership boundary.",
                mime_type="text/markdown",
            ),
            types.Resource(
                uri=AGENTS_URI,
                name="agents",
                title="Anonymous simulation fixture agent manual",
                description="Implementation invariants and compliance declaration.",
                mime_type="text/markdown",
            ),
            types.Resource(
                uri=CONTRACT_URI,
                name="contract",
                title="Anonymous simulation fixture contract",
                description=(
                    "Machine-readable contract revision, compliance, and "
                    "capability inventory."
                ),
                mime_type="application/json",
            ),
        ]
        return types.ListResourcesResult(resources=resources)

    async def read_resource(
        ctx: Context, params: types.ReadResourceRequestParams
    ) -> types.ReadResourceResult:
        text = params.uri
        _identity(scope(ctx))
        if text == STATE_URI:
            state = await runtime.fixture_state()
            return _json_result(
                text, state.model_dump(mode="json", by_alias=True)
            )
        if text == DOCS_URI:
            return _json_result(text, list(DOCS_INDEX))
        if text == DESIGN_URI:
            return _markdown_result(text, DESIGN_DOCUMENT.body)
        if text == AGENTS_URI:
            return _markdown_result(text, AGENTS_DOCUMENT.body)
        if text == CONTRACT_URI:
            return _json_result(text, CONTRACT_DECLARATION.wire())
        raise _invalid(f"unknown resource URI `{text}`")

    return Server(
        "anonymous-simulation",
        version="0.1.0",
        instructions=INSTRUCTIONS,
        on_list_tools=list_tools,
        on_call_tool=call_tool,
        on_list_resources=list_resources,
        on_read_resource=read_resource,
    )


def _identity(scope: dict[str, Any]) -> GatewayInternalIdentity:
    identity = scope.get(IDENTITY_SCOPE_KEY)
    if not isinstance(identity, GatewayInternalIdentity):
        raise _invalid("gateway identity missing")
    return identity


def _caller(scope: dict[str, Any]) -> PlaneCaller:
    identity = _identity(scope)
    bearer = scope.get(BEARER_SCOPE_KEY)
    if not isinstance(bearer, str):
        raise _invalid("forwarded bearer missing")
    return PlaneCaller.from_identity(identity, bearer)


def _structured(text: str, output: Any) -> types.CallToolResult:
    return types.CallToolResult(
        content=[types.TextContent(type="text", text=text)],
        structured_content=output.model_dump(
            mode="json",
            by_alias=True,
            exclude_none=True,
        ),
        is_error=False,
    )


def _error_result(message: str) -> types.CallToolResult:
    return types.CallToolResult(
        content=[types.TextContent(type="text", text=message)],
        is_error=True,
    )


def _json_result(uri: str, value: object) -> types.ReadResourceResult:
    return types.ReadResourceResult(
        contents=[
            types.TextResourceContents(
                uri=uri,
                text=json.dumps(value, separators=(",", ":")),
                mime_type="application/json",
            )
        ]
    )


def _markdown_result(uri: str, body: str) -> types.ReadResourceResult:
    return types.ReadResourceResult(
        contents=[
            types.TextResourceContents(
                uri=uri, text=body, mime_type="text/markdown"
            )
        ]
    )


def _invalid(message: str) -> MCPError:
    return MCPError(code=types.INVALID_REQUEST, message=message)
