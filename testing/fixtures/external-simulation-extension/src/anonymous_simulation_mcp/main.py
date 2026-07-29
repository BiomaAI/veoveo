"""Streamable HTTP entrypoint for the anonymous external fixture."""

from __future__ import annotations

import asyncio
import contextlib
from typing import Any, Awaitable, Callable

import uvicorn
from mcp.server.streamable_http_manager import StreamableHTTPSessionManager

from veoveo_mcp.contract import GATEWAY_INTERNAL_TOKEN_ISSUER
from veoveo_mcp.host import HostValidationMiddleware
from veoveo_mcp.internal_auth import (
    GatewayInternalTokenVerifier,
    GatewayInternalTrustBundle,
    InternalAuthMiddleware,
)
from veoveo_mcp.telemetry import JsonLogger

from .config import SERVER_SLUG, Config, parse_config
from .mcp_server import LLMS_TXT, SERVER_DOCS, build_mcp_server
from .runtime import FixtureRuntime


SERVICE_NAME = "anonymous-simulation-mcp"
AsgiApp = Callable[..., Awaitable[None]]


class RootApp:
    def __init__(
        self,
        protected_app: AsgiApp,
        session_manager: StreamableHTTPSessionManager,
        ready: asyncio.Event,
    ) -> None:
        self._protected_app = protected_app
        self._session_manager = session_manager
        self._ready = ready

    async def __call__(self, scope: dict[str, Any], receive, send) -> None:
        if scope["type"] == "lifespan":
            await self._lifespan(receive, send)
            return
        if scope["type"] != "http":
            return
        path = scope.get("path", "")
        if path == "/anonymous-simulation/healthz":
            await _plain(send, 200, b"ok")
        elif path == "/anonymous-simulation/readyz":
            await _plain(send, 200 if self._ready.is_set() else 503, b"ok")
        elif path.startswith("/anonymous-simulation/mcp") or path.startswith(
            "/anonymous-simulation/admin/"
        ):
            await self._protected_app(scope, receive, send)
        else:
            await _plain(send, 404, b"not found")

    async def _lifespan(self, receive, send) -> None:
        async with contextlib.AsyncExitStack() as stack:
            while True:
                message = await receive()
                if message["type"] == "lifespan.startup":
                    try:
                        await stack.enter_async_context(self._session_manager.run())
                        self._ready.set()
                    except Exception as error:  # noqa: BLE001
                        await send(
                            {
                                "type": "lifespan.startup.failed",
                                "message": str(error),
                            }
                        )
                        return
                    await send({"type": "lifespan.startup.complete"})
                elif message["type"] == "lifespan.shutdown":
                    self._ready.clear()
                    await send({"type": "lifespan.shutdown.complete"})
                    return


async def _plain(send, status: int, body: bytes) -> None:
    await send(
        {
            "type": "http.response.start",
            "status": status,
            "headers": [
                (b"content-type", b"text/plain; charset=utf-8"),
                (b"content-length", str(len(body)).encode()),
            ],
        }
    )
    await send({"type": "http.response.body", "body": body})


async def _markdown(send, content: str) -> None:
    body = content.encode()
    await send(
        {
            "type": "http.response.start",
            "status": 200,
            "headers": [
                (b"content-type", b"text/markdown; charset=utf-8"),
                (b"content-length", str(len(body)).encode()),
            ],
        }
    )
    await send({"type": "http.response.body", "body": body})


async def serve(config: Config) -> None:
    logger = JsonLogger(SERVICE_NAME)
    runtime = FixtureRuntime(config)
    mcp_server = build_mcp_server(runtime)
    session_manager = StreamableHTTPSessionManager(
        app=mcp_server,
        json_response=False,
        stateless=False,
        session_idle_timeout=60,
    )

    async def protected_asgi(scope, receive, send):
        path = scope.get("path", "")
        if path == "/anonymous-simulation/admin/docs/llms.txt":
            await _plain(send, 200, LLMS_TXT.encode())
        elif path.startswith("/anonymous-simulation/admin/docs/"):
            doc_id = path.removeprefix(
                "/anonymous-simulation/admin/docs/"
            )
            doc = SERVER_DOCS.doc(doc_id)
            if doc is None:
                await _plain(send, 404, b"unknown server document")
            else:
                await _markdown(send, doc.body)
        elif path == "/anonymous-simulation/mcp" or path.startswith(
            "/anonymous-simulation/mcp/"
        ):
            await session_manager.handle_request(scope, receive, send)
        else:
            await _plain(send, 404, b"not found")

    verifier = GatewayInternalTokenVerifier.for_server(
        GATEWAY_INTERNAL_TOKEN_ISSUER,
        SERVER_SLUG,
        GatewayInternalTrustBundle.from_json(config.internal_trust_jwks),
    )
    protected_stack = InternalAuthMiddleware(protected_asgi, verifier, logger.warn)
    ready = asyncio.Event()
    app = HostValidationMiddleware(
        RootApp(protected_stack, session_manager, ready),
        list(config.allowed_hosts),
        logger.warn,
    )
    server = uvicorn.Server(
        uvicorn.Config(
            app,
            host="0.0.0.0",
            port=config.port,
            log_level="warning",
            lifespan="on",
        )
    )
    logger.info(
        "listening",
        address=f"0.0.0.0:{config.port}",
        mcp_path="/anonymous-simulation/mcp",
    )
    try:
        await server.serve()
    finally:
        await runtime.close()


def main() -> None:
    asyncio.run(serve(parse_config()))


if __name__ == "__main__":
    main()
