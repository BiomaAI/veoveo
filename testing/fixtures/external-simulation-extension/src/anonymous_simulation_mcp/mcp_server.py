"""Hosted MCP surface for assets and the synthetic pose producer."""

from __future__ import annotations

import json
from typing import Any

import mcp.types as types
from mcp.server.lowlevel import Server
from mcp.server.lowlevel.helper_types import ReadResourceContents
from mcp.shared.exceptions import McpError
from pydantic import ValidationError

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
DOC_INDEX = """# Anonymous Simulation Fixture

- [Design](anonymous-simulation://docs/design)
- [Agent manual](anonymous-simulation://docs/agents)
"""
DESIGN_DOCUMENT = """# Anonymous Simulation Extension

## Standards And Protocols

This fixture uses MCP Streamable HTTP, OpenUSD USDA, the Veoveo Simulation
View scene contract, and the Veoveo latest-pose mTLS data plane.

It owns declarative synthetic assets and complete moving-entity pose
snapshots. Simulation View owns scene mirroring, cameras, RTX rendering,
NVENC, WebRTC, capacity, leases, and the live MCP App.
"""
AGENTS_DOCUMENT = """# Anonymous Simulation MCP Server — Agent Manual

Contract revision: 2

Do not add camera, renderer, media, live-view, dynamics, or provider
implementation logic. Publish governed assets and typed latest poses only.
"""


def build_mcp_server(runtime: FixtureRuntime) -> Server:
    server: Server = Server(
        "anonymous-simulation",
        version="0.1.0",
        instructions=INSTRUCTIONS,
    )

    def scope() -> dict[str, Any]:
        request = server.request_context.request
        if request is None:
            raise _invalid("authenticated HTTP context missing")
        return request.scope

    @server.list_tools()
    async def list_tools() -> list[types.Tool]:
        return [
            types.Tool(
                name="prepare_scene",
                title="Prepare synthetic Simulation View scene",
                description=(
                    "Publish fixture-owned OpenUSD assets to the Artifact plane "
                    "and return one immutable Simulation View scene declaration."
                ),
                inputSchema=mcp_input_schema(PrepareSceneRequest),
                outputSchema=PreparedScene.model_json_schema(),
                annotations=types.ToolAnnotations(
                    readOnlyHint=False,
                    destructiveHint=False,
                    idempotentHint=False,
                    openWorldHint=False,
                ),
            ),
            types.Tool(
                name="start_pose_producer",
                title="Start synthetic pose producer",
                description=(
                    "Start complete moving-entity snapshots on the independent "
                    "mTLS Simulation View pose data plane."
                ),
                inputSchema=mcp_input_schema(StartPoseProducerRequest),
                outputSchema=ProducerState.model_json_schema(),
                annotations=types.ToolAnnotations(
                    readOnlyHint=False,
                    destructiveHint=False,
                    idempotentHint=True,
                    openWorldHint=False,
                ),
            ),
            types.Tool(
                name="stop_pose_producer",
                title="Stop synthetic pose producer",
                description="Stop the fixture-owned pose stream.",
                inputSchema=mcp_input_schema(StopPoseProducerRequest),
                outputSchema=ProducerState.model_json_schema(),
                annotations=types.ToolAnnotations(
                    readOnlyHint=False,
                    destructiveHint=True,
                    idempotentHint=True,
                    openWorldHint=False,
                ),
            ),
            types.Tool(
                name="get_fixture_state",
                title="Read synthetic producer state",
                description="Read redacted pose-producer lifecycle and counters.",
                inputSchema=mcp_input_schema(GetFixtureStateRequest),
                outputSchema=FixtureState.model_json_schema(),
                annotations=types.ToolAnnotations(
                    readOnlyHint=True,
                    destructiveHint=False,
                    idempotentHint=True,
                    openWorldHint=False,
                ),
            ),
        ]

    @server.call_tool()
    async def call_tool(
        name: str,
        arguments: dict[str, Any],
    ) -> types.CallToolResult:
        try:
            if name == "prepare_scene":
                request = PrepareSceneRequest.model_validate(arguments)
                output = await runtime.prepare_scene(_caller(scope()), request)
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
        except (ValidationError, ValueError) as error:
            raise _invalid(str(error)) from error
        raise _invalid(f"unknown tool `{name}`")

    @server.list_resources()
    async def list_resources(
        _request: types.ListResourcesRequest,
    ) -> types.ListResourcesResult:
        resources = [
            types.Resource(
                uri=STATE_URI,
                name="state",
                title="Anonymous simulation fixture state",
                description="Redacted synthetic pose-producer state.",
                mimeType="application/json",
            ),
            types.Resource(
                uri=DOCS_URI,
                name="docs",
                title="Anonymous simulation fixture documentation",
                description="Index of embedded fixture documents.",
                mimeType="text/markdown",
            ),
            types.Resource(
                uri=DESIGN_URI,
                name="design",
                title="Anonymous simulation fixture design",
                description="Public protocol and ownership boundary.",
                mimeType="text/markdown",
            ),
            types.Resource(
                uri=AGENTS_URI,
                name="agents",
                title="Anonymous simulation fixture agent manual",
                description="Implementation invariants and compliance declaration.",
                mimeType="text/markdown",
            ),
            types.Resource(
                uri=CONTRACT_URI,
                name="contract",
                title="Anonymous simulation fixture contract",
                description="Canonical fixture schemas and identities.",
                mimeType="application/json",
            ),
        ]
        return types.ListResourcesResult(resources=resources)

    @server.read_resource()
    async def read_resource(uri: Any) -> list[ReadResourceContents]:
        text = str(uri)
        _identity(scope())
        if text == STATE_URI:
            state = await runtime.fixture_state()
            return [_json_contents(state.model_dump(mode="json", by_alias=True))]
        if text == DOCS_URI:
            return [
                ReadResourceContents(
                    content=DOC_INDEX,
                    mime_type="text/markdown",
                )
            ]
        if text == DESIGN_URI:
            return [
                ReadResourceContents(
                    content=DESIGN_DOCUMENT,
                    mime_type="text/markdown",
                )
            ]
        if text == AGENTS_URI:
            return [
                ReadResourceContents(
                    content=AGENTS_DOCUMENT,
                    mime_type="text/markdown",
                )
            ]
        if text == CONTRACT_URI:
            return [
                _json_contents(
                    {
                        "schemaVersion": "veoveo.io/hosted-mcp-contract/v2",
                        "contractRevision": "2",
                        "server": "anonymous-simulation",
                        "sceneSchema": "veoveo.io/simulation-view-scene/v1",
                        "poseSchema": "veoveo.io/simulation-view-pose/v1",
                        "capabilities": {
                            "tools": True,
                            "resources": True,
                            "resourceTemplates": False,
                            "resourceSubscriptions": False,
                            "prompts": False,
                            "completions": False,
                            "tasks": False,
                        },
                        "resources": [
                            STATE_URI,
                            DOCS_URI,
                            DESIGN_URI,
                            AGENTS_URI,
                            CONTRACT_URI,
                        ],
                    }
                )
            ]
        raise _invalid(f"unknown resource URI `{text}`")

    return server


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
        structuredContent=output.model_dump(
            mode="json",
            by_alias=True,
            exclude_none=True,
        ),
        isError=False,
    )


def _json_contents(value: object) -> ReadResourceContents:
    return ReadResourceContents(
        content=json.dumps(value, separators=(",", ":")),
        mime_type="application/json",
    )


def _invalid(message: str) -> McpError:
    return McpError(types.ErrorData(code=types.INVALID_REQUEST, message=message))
