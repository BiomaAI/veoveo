"""The container runtime: one process, one TraCI owner, one clock.

Builds the sim driver (real TraCI when SUMO_HOST is set, else the fake world),
optionally starts a background push loop that streams world state into the hub,
and serves the governed MCP tools over streamable HTTP. The push loop and the
MCP tools both go through the toolset's lock, so the single sim owner is never
accessed concurrently.
"""

from __future__ import annotations

import contextlib
import os

from .sim_driver import FakeSimDriver, SimDriver
from .tools import SumoToolset


def build_driver() -> SimDriver:
    host = os.environ.get("SUMO_HOST")
    if host:
        from .sim_driver import TraciSimDriver

        port = int(os.environ.get("SUMO_PORT", "8813"))
        return TraciSimDriver(host=host, port=port, name=os.environ.get("SUMO_SCENARIO", "sumo"))
    return FakeSimDriver(
        n_vehicles=int(os.environ.get("SUMO_FAKE_VEHICLES", "12")),
        seed=int(os.environ.get("SUMO_FAKE_SEED", "1")),
    )


async def push_loop(toolset: SumoToolset, proxy: str, recording: str, period_s: float) -> None:
    """Step the sim and publish each frame into the hub, forever."""
    import anyio

    from .streams import RerunPublisher

    publisher = RerunPublisher(proxy, application_id="veoveo-sumo", recording=recording)
    step = 0
    try:
        while True:
            vehicles, signals, mean_speed = await toolset.step_once()
            publisher.publish(step, vehicles, signals, mean_speed)
            step += 1
            await anyio.sleep(period_s)
    finally:
        with contextlib.suppress(Exception):
            publisher.flush()


def main() -> None:  # pragma: no cover - real serving path
    import argparse

    import anyio
    import uvicorn
    from mcp.server.streamable_http_manager import StreamableHTTPSessionManager
    from starlette.applications import Starlette
    from starlette.routing import Mount

    from .server import build_server

    parser = argparse.ArgumentParser(description="SUMO MCP runtime (serve + push)")
    parser.add_argument("--host", default="0.0.0.0")
    parser.add_argument("--port", type=int, default=8795)
    parser.add_argument("--hub-proxy", default=os.environ.get("HUB_PROXY", ""))
    parser.add_argument("--recording", default=os.environ.get("SUMO_RECORDING", "sumo-run"))
    parser.add_argument("--push-period-s", type=float, default=0.5)
    args = parser.parse_args()

    driver = build_driver()
    toolset = SumoToolset(driver)
    server = build_server(toolset)
    manager = StreamableHTTPSessionManager(app=server)

    async def handle(scope, receive, send):  # type: ignore[no-untyped-def]
        await manager.handle_request(scope, receive, send)

    @contextlib.asynccontextmanager
    async def lifespan(app):  # type: ignore[no-untyped-def]
        async with manager.run():
            async with anyio.create_task_group() as tg:
                if args.hub_proxy:
                    tg.start_soon(
                        push_loop, toolset, args.hub_proxy, args.recording, args.push_period_s
                    )
                yield
                tg.cancel_scope.cancel()

    app = Starlette(routes=[Mount("/mcp", app=handle)], lifespan=lifespan)
    uvicorn.run(app, host=args.host, port=args.port)


if __name__ == "__main__":  # pragma: no cover
    main()
