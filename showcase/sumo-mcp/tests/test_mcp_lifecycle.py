"""End-to-end MCP lifecycle over an in-memory client<->server session.

Proves the governed surface the gateway would project: tool listing with task
modes, a synchronous call, and the full task path — call_tool_as_task returns a
CreateTaskResult, the client polls, and get_task_result returns the terminal
typed result. This is the exact detach/sleep/wake path the agent kernel drives,
minus the network.
"""

from __future__ import annotations

import contextlib

import anyio
from mcp import types
from mcp.client.experimental.task_handlers import ExperimentalTaskHandlers
from mcp.client.session import ClientSession
from mcp.shared.memory import create_client_server_memory_streams

from sumo_mcp.server import build_server
from sumo_mcp.sim_driver import FakeSimDriver
from sumo_mcp.tools import SumoToolset


def make_server():
    driver = FakeSimDriver(n_vehicles=6, seed=5, congestion_window=(3, 8))
    return build_server(SumoToolset(driver))


@contextlib.asynccontextmanager
async def client_session():
    """In-memory client<->server session with client task support declared.

    Replicates mcp.shared.memory but passes experimental_task_handlers, which the
    stock helper does not forward — required to invoke TASK_REQUIRED tools.
    """
    server = make_server()
    async with create_client_server_memory_streams() as (client_streams, server_streams):
        client_read, client_write = client_streams
        server_read, server_write = server_streams
        async with anyio.create_task_group() as tg:
            tg.start_soon(
                lambda: server.run(
                    server_read,
                    server_write,
                    server.create_initialization_options(),
                    raise_exceptions=True,
                )
            )
            async with ClientSession(
                read_stream=client_read,
                write_stream=client_write,
                experimental_task_handlers=ExperimentalTaskHandlers(),
            ) as client:
                await client.initialize()
                yield client
            tg.cancel_scope.cancel()


async def test_list_tools_advertises_task_modes() -> None:
    async with client_session() as client:
        tools = (await client.list_tools()).tools
        by_name = {t.name: t for t in tools}
        assert "query_state" in by_name
        assert "run_batch" in by_name
        # Sync tools forbid task augmentation; long ops require it.
        assert by_name["query_state"].execution.taskSupport == types.TASK_FORBIDDEN
        assert by_name["run_batch"].execution.taskSupport == types.TASK_REQUIRED
        assert by_name["optimize_signals"].execution.taskSupport == types.TASK_REQUIRED


async def test_sync_tool_returns_structured_state() -> None:
    async with client_session() as client:
        result = await client.call_tool("query_state", {})
        assert not result.isError
        data = result.structuredContent
        assert data is not None
        assert data["vehicle_count"] == 6
        assert len(data["vehicles"]) == 6


async def test_run_batch_task_detach_poll_result() -> None:
    async with client_session() as client:

        # Detach: the server returns a task, not the immediate result.
        created = await client.experimental.call_tool_as_task(
            "run_batch", {"steps": 10}, ttl=60_000
        )
        task_id = created.task.taskId
        assert created.task.status in ("working", "completed")

        # Poll to terminal (the agent would sleep here and wake on notification).
        async for snapshot in client.experimental.poll_task(task_id):
            if snapshot.status in ("completed", "failed", "cancelled"):
                break
        assert snapshot.status == "completed"

        # tasks/result blocks until terminal, returns the typed CallToolResult.
        payload = await client.experimental.get_task_result(task_id, types.CallToolResult)
        assert not payload.isError
        body = payload.structuredContent
        assert body["steps_advanced"] == 10
        # Jam window [3,8) guarantees congestion was detected during the batch.
        assert body["congestion_detected"] is True


async def test_offline_generator_task() -> None:
    async with client_session() as client:
        created = await client.experimental.call_tool_as_task(
            "generate_network", {"kind": "grid", "seed": 1}
        )
        tid = created.task.taskId
        async for snap in client.experimental.poll_task(tid):
            if snap.status in ("completed", "failed", "cancelled"):
                break
        assert snap.status == "completed"
        payload = await client.experimental.get_task_result(tid, types.CallToolResult)
        assert payload.structuredContent["artifact"] == "generate_network-grid-1.xml"


async def test_task_required_tool_rejects_plain_call() -> None:
    # run_batch is TASK_REQUIRED: a non-task call must be rejected.
    async with client_session() as client:
        result = await client.call_tool("run_batch", {"steps": 5})
        assert result.isError
