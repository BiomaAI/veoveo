"""Event plane: the congestion watch condition as a subscribable resource.

Proves the wake path: a client subscribes to sim://congestion, the sim advances
into a jam, check_events pushes resources/updated, and the client's message
handler receives the notification — exactly what the kernel turns into a wake.
"""

from __future__ import annotations

import contextlib

import anyio
from mcp import types
from mcp.client.session import ClientSession
from mcp.shared.memory import create_client_server_memory_streams
from mcp.shared.session import RequestResponder

from sumo_mcp.resources import CONGESTION_URI
from sumo_mcp.server import build_server
from sumo_mcp.sim_driver import FakeSimDriver
from sumo_mcp.tools import SumoToolset


def make_server():
    # Congestion from the very first step, so the batch reliably trips it.
    driver = FakeSimDriver(n_vehicles=6, seed=5, congestion_window=(0, 100))
    return build_server(SumoToolset(driver))


@contextlib.asynccontextmanager
async def client_with_notifications(received: list[types.ServerNotification]):
    async def message_handler(
        message: RequestResponder[types.ServerRequest, types.ClientResult]
        | types.ServerNotification
        | Exception,
    ) -> None:
        if isinstance(message, types.ServerNotification):
            received.append(message)

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
                message_handler=message_handler,
            ) as client:
                await client.initialize()
                yield client
            tg.cancel_scope.cancel()


async def test_congestion_resource_is_listed_and_readable() -> None:
    received: list[types.ServerNotification] = []
    async with client_with_notifications(received) as client:
        resources = (await client.list_resources()).resources
        assert any(str(r.uri) == CONGESTION_URI for r in resources)
        content = await client.read_resource(CONGESTION_URI)
        assert content.contents  # current congestion state as JSON


async def test_subscribe_then_congestion_pushes_resource_updated() -> None:
    received: list[types.ServerNotification] = []
    async with client_with_notifications(received) as client:
        await client.subscribe_resource(CONGESTION_URI)

        # check_events evaluates the (always-jammed) world and must push an update.
        result = await client.call_tool("check_events", {})
        assert result.structuredContent["congested"] is True

        # Give the notification a moment to arrive, then assert we were woken.
        with anyio.move_on_after(2.0):
            while not received:
                await anyio.sleep(0.02)

        updates = [
            n
            for n in received
            if isinstance(n.root, types.ResourceUpdatedNotification)
            and str(n.root.params.uri) == CONGESTION_URI
        ]
        assert updates, f"expected a resources/updated for congestion, got {received}"
