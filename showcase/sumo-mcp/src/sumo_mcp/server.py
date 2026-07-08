"""The task-native SUMO MCP server.

A lowlevel MCP server (the tasks API lives only there, not on FastMCP) that
projects the SUMO world as governed tools. Synchronous tools read the latest
state and actuate; the long operations — run_batch and the offline generators —
are MCP *tasks*: the handler returns a CreateTaskResult and the client detaches,
polls, and reads the terminal result, which is exactly the sleep/wake path the
agent kernel drives. `build_server` is separable so tests exercise the whole
lifecycle in-process over memory streams; `main` serves it over streamable HTTP.
"""

from __future__ import annotations

import json
from collections.abc import Awaitable, Callable
from dataclasses import dataclass

from mcp import types
from mcp.server.lowlevel import Server
from pydantic import BaseModel

from pydantic import AnyUrl

from . import tasks_compat
from .resources import CONGESTION_URI, evaluate_congestion
from .sim_driver import FakeSimDriver, SimDriver
from .tools import (
    OfflineOpParams,
    RerouteVehicleParams,
    RunBatchParams,
    SetSignalPhaseParams,
    SumoToolset,
    _Model,
)


@dataclass(frozen=True)
class ToolSpec:
    name: str
    description: str
    params_model: type[_Model] | None
    task_mode: str  # tasks_compat.TASK_FORBIDDEN | TASK_REQUIRED
    # Returns a pydantic result model (sync) — task tools are dispatched separately.


def _schema(model: type[_Model] | None) -> dict:
    if model is None:
        return {"type": "object", "properties": {}, "additionalProperties": False}
    return model.model_json_schema()


def _ok(result: BaseModel) -> types.CallToolResult:
    payload = result.model_dump()
    return types.CallToolResult(
        content=[types.TextContent(type="text", text=json.dumps(payload))],
        structuredContent=payload,
    )


# Tool taxonomy: synchronous reads/actuation vs task-native long operations.
SYNC_TOOLS: dict[str, ToolSpec] = {
    "query_state": ToolSpec(
        "query_state",
        "Live traffic state: vehicles (geo + speed), signals, mean speed.",
        None,
        tasks_compat.TASK_FORBIDDEN,
    ),
    "describe_scenario": ToolSpec(
        "describe_scenario",
        "Loaded network: edges, signals, geo origin.",
        None,
        tasks_compat.TASK_FORBIDDEN,
    ),
    "set_signal_phase": ToolSpec(
        "set_signal_phase",
        "Set a traffic signal phase (applied at the next safe step).",
        SetSignalPhaseParams,
        tasks_compat.TASK_FORBIDDEN,
    ),
    "reroute_vehicle": ToolSpec(
        "reroute_vehicle",
        "Reroute a vehicle onto a target edge.",
        RerouteVehicleParams,
        tasks_compat.TASK_FORBIDDEN,
    ),
    "check_events": ToolSpec(
        "check_events",
        "Evaluate watch conditions (congestion) and push resources/updated for "
        "any that crossed threshold, so subscribers are woken.",
        None,
        tasks_compat.TASK_FORBIDDEN,
    ),
}

TASK_TOOLS: dict[str, ToolSpec] = {
    "run_batch": ToolSpec(
        "run_batch",
        "Advance the simulation N steps and return aggregate outcomes. "
        "Long op: invoke as an MCP task and read tasks/result.",
        RunBatchParams,
        tasks_compat.TASK_REQUIRED,
    ),
    "generate_network": ToolSpec(
        "generate_network",
        "Generate a SUMO network (grid/spider/osm). Long op: MCP task.",
        OfflineOpParams,
        tasks_compat.TASK_REQUIRED,
    ),
    "compute_routes": ToolSpec(
        "compute_routes",
        "Compute demand/routes for the scenario. Long op: MCP task.",
        OfflineOpParams,
        tasks_compat.TASK_REQUIRED,
    ),
    "optimize_signals": ToolSpec(
        "optimize_signals",
        "Optimize traffic-signal timing. Long op: MCP task.",
        OfflineOpParams,
        tasks_compat.TASK_REQUIRED,
    ),
}


def build_server(toolset: SumoToolset, name: str = "veoveo-sumo-mcp") -> Server:
    server: Server = Server(name)
    tasks_compat.enable_tasks(server)

    # --- event plane: the congestion watch condition as a resource ----------
    @server.list_resources()
    async def list_resources() -> list[types.Resource]:  # type: ignore[no-untyped-def]
        return [
            types.Resource(
                uri=AnyUrl(CONGESTION_URI),
                name="congestion",
                description="Network congestion watch condition (mean speed threshold).",
                mimeType="application/json",
            )
        ]

    @server.read_resource()
    async def read_resource(uri: AnyUrl) -> str:  # type: ignore[no-untyped-def]
        if str(uri) == CONGESTION_URI:
            return evaluate_congestion(toolset.driver).model_dump_json()
        raise ValueError(f"unknown resource {uri}")

    @server.subscribe_resource()
    async def subscribe_resource(uri: AnyUrl) -> None:  # type: ignore[no-untyped-def]
        return None

    @server.unsubscribe_resource()
    async def unsubscribe_resource(uri: AnyUrl) -> None:  # type: ignore[no-untyped-def]
        return None

    @server.list_tools()
    async def list_tools() -> list[types.Tool]:  # type: ignore[no-untyped-def]
        tools: list[types.Tool] = []
        for spec in {**SYNC_TOOLS, **TASK_TOOLS}.values():
            tools.append(
                types.Tool(
                    name=spec.name,
                    description=spec.description,
                    inputSchema=_schema(spec.params_model),
                    execution=tasks_compat.tool_execution(spec.task_mode),
                )
            )
        return tools

    @server.call_tool()
    async def call_tool(  # type: ignore[no-untyped-def]
        name: str, arguments: dict
    ) -> types.CallToolResult | types.CreateTaskResult:
        # --- synchronous tools ---------------------------------------------
        if name in SYNC_TOOLS:
            SYNC_TOOLS[name].params_model  # noqa: B018 (schema already validated)
            if name == "query_state":
                return _ok(await toolset.query_state())
            if name == "describe_scenario":
                return _ok(await toolset.describe_scenario())
            if name == "set_signal_phase":
                return _ok(await toolset.set_signal_phase(SetSignalPhaseParams(**arguments)))
            if name == "reroute_vehicle":
                return _ok(await toolset.reroute_vehicle(RerouteVehicleParams(**arguments)))
            if name == "check_events":
                state = evaluate_congestion(toolset.driver)
                if state.congested:
                    # Push a wake to subscribers of the congestion resource.
                    await server.request_context.session.send_resource_updated(
                        AnyUrl(CONGESTION_URI)
                    )
                return _ok(state)

        # --- task-native long operations -----------------------------------
        if name in TASK_TOOLS:
            work = _task_work(toolset, name, arguments)
            return await tasks_compat.run_as_task(server, work)

        raise ValueError(f"unknown tool {name}")

    return server


def _task_work(
    toolset: SumoToolset, name: str, arguments: dict
) -> Callable[[object], Awaitable[types.CallToolResult]]:
    async def work(task_ctx: object) -> types.CallToolResult:
        # Progress notifications are best-effort; the terminal result is what
        # the agent wakes on.
        update = getattr(task_ctx, "update_status", None)
        if name == "run_batch":
            if callable(update):
                await update(f"running {arguments.get('steps')} steps")
            return _ok(await toolset.run_batch(RunBatchParams(**arguments)))
        # offline generators
        if callable(update):
            await update(f"{name} working")
        return _ok(await toolset.offline_op(name, OfflineOpParams(**arguments)))

    return work


def main() -> None:  # pragma: no cover - real serving path
    """Serve over streamable HTTP (the gateway upstream)."""
    import argparse

    import uvicorn
    from mcp.server.streamable_http_manager import StreamableHTTPSessionManager
    from starlette.applications import Starlette
    from starlette.routing import Mount

    parser = argparse.ArgumentParser(description="SUMO MCP server")
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=8795)
    args = parser.parse_args()

    driver: SimDriver = FakeSimDriver()
    toolset = SumoToolset(driver)
    server = build_server(toolset)

    manager = StreamableHTTPSessionManager(app=server)

    async def handle(scope, receive, send):  # type: ignore[no-untyped-def]
        await manager.handle_request(scope, receive, send)

    import contextlib

    @contextlib.asynccontextmanager
    async def lifespan(app):  # type: ignore[no-untyped-def]
        async with manager.run():
            yield

    app = Starlette(routes=[Mount("/mcp", app=handle)], lifespan=lifespan)
    uvicorn.run(app, host=args.host, port=args.port)


if __name__ == "__main__":  # pragma: no cover
    main()
