"""Datasheet MCP server entrypoint.

MCP surface:
  tools for dataset preview and column statistics
  task-required `profile_dataset`
  templates for per-task usage and shared-plane artifacts

Middleware order matches the Rust hosted servers: host validation outermost,
then the gateway internal-auth requirement, then the final task extension,
then the streamable HTTP MCP session.
"""

from __future__ import annotations

import asyncio
import contextlib
import uuid
from typing import Any, Awaitable, Callable

import uvicorn
from mcp.server.streamable_http_manager import StreamableHTTPSessionManager

from veoveo_mcp.artifacts import ArtifactRepository
from veoveo_mcp.contract import GATEWAY_INTERNAL_TOKEN_ISSUER
from veoveo_mcp.deployment import public_allowed_hosts
from veoveo_mcp.host import HostValidationMiddleware
from veoveo_mcp.internal_auth import (
    GatewayInternalTokenVerifier,
    GatewayInternalTrustBundle,
    InternalAuthMiddleware,
)
from veoveo_mcp.task_extension import (
    Implementation,
    ServerDiscovery,
    TaskExtensionMiddleware,
)
from veoveo_mcp.tasks import TaskRuntime
from veoveo_mcp.telemetry import JsonLogger

from .. import uris
from ..docs import LLMS_TXT, SERVER_DOCS
from .app_state import AppState
from .config import Config, parse_config
from .mcp_server import INSTRUCTIONS, build_mcp_server
from .profile_task import SERVER_SLUG, resume_profile_tasks
from .task_extension import DatasheetTaskExtension

SERVICE_NAME = "veoveo-datasheet-mcp"

AsgiApp = Callable[..., Awaitable[None]]


class RootApp:
    """Exact-path ASGI router with the session-manager lifespan.

    The MCP mount must not redirect: clients POST exactly to `/{slug}/mcp`,
    and every response — including 401 and 421 — must come from the
    middleware stack, never from routing.
    """

    def __init__(
        self,
        health_path: str,
        ready_path: str,
        docs_llms_path: str,
        docs_prefix: str,
        mcp_path: str,
        protected_app: AsgiApp,
        session_manager: StreamableHTTPSessionManager,
        ready: asyncio.Event,
    ) -> None:
        self.health_path = health_path
        self.ready_path = ready_path
        self.docs_llms_path = docs_llms_path
        self.docs_prefix = docs_prefix
        self.mcp_path = mcp_path
        self.protected_app = protected_app
        self.session_manager = session_manager
        self.ready = ready

    async def __call__(self, scope: dict[str, Any], receive, send) -> None:
        if scope["type"] == "lifespan":
            await self._lifespan(receive, send)
            return
        if scope["type"] != "http":
            return
        path = scope.get("path", "")
        if path == self.health_path:
            await _plain(send, 200, b"ok")
            return
        if path == self.ready_path:
            if self.ready.is_set():
                await _plain(send, 200, b"ok")
            else:
                await _plain(send, 503, b"starting")
            return
        if path == self.docs_llms_path:
            await self.protected_app(scope, receive, send)
            return
        if path.startswith(self.docs_prefix):
            await self.protected_app(scope, receive, send)
            return
        if path == self.mcp_path or path.startswith(f"{self.mcp_path}/"):
            await self.protected_app(scope, receive, send)
            return
        await _plain(send, 404, b"not found")

    async def _lifespan(self, receive, send) -> None:
        async with contextlib.AsyncExitStack() as stack:
            while True:
                message = await receive()
                if message["type"] == "lifespan.startup":
                    try:
                        await stack.enter_async_context(self.session_manager.run())
                        self.ready.set()
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
                    await send({"type": "lifespan.shutdown.complete"})
                    return


async def _markdown(send, body: str) -> None:
    encoded = body.encode()
    await send(
        {
            "type": "http.response.start",
            "status": 200,
            "headers": [
                (b"content-type", b"text/markdown; charset=utf-8"),
                (b"content-length", str(len(encoded)).encode()),
            ],
        }
    )
    await send({"type": "http.response.body", "body": encoded})


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


async def serve(config: Config) -> None:
    logger = JsonLogger(SERVICE_NAME)
    deployment = config.public_deployment()
    endpoint = deployment.server(SERVER_SLUG)
    verifier = GatewayInternalTokenVerifier.for_server(
        GATEWAY_INTERNAL_TOKEN_ISSUER,
        SERVER_SLUG,
        GatewayInternalTrustBundle.from_json(config.internal_trust_jwks),
    )
    tasks = await TaskRuntime.connect(
        config.surreal_endpoint,
        config.surreal_namespace,
        config.surreal_database,
        config.surreal_username,
        config.surreal_password,
        SERVER_SLUG,
        f"{SERVER_SLUG}-{uuid.uuid4()}",
    )
    state = AppState(
        tasks=tasks,
        artifacts=ArtifactRepository(config.artifact_service_url, uris.SCHEME),
        logger=logger,
        max_artifact_bytes=config.max_artifact_bytes,
        max_dataset_bytes=config.max_dataset_bytes,
    )
    resumed = await resume_profile_tasks(state)
    if resumed:
        logger.info("resumed recovered datasheet tasks", count=resumed)

    mcp_server = build_mcp_server(state)
    session_manager = StreamableHTTPSessionManager(
        app=mcp_server,
        json_response=False,
        stateless=False,
        session_idle_timeout=60,
    )

    async def mcp_asgi(scope, receive, send):
        await session_manager.handle_request(scope, receive, send)

    discovery = ServerDiscovery(
        capabilities={
            "tools": {},
            "resources": {},
            "prompts": {},
            "completions": {},
        },
        server_info=Implementation(name="datasheet", version="0.1.0"),
        instructions=INSTRUCTIONS,
    )
    task_app = TaskExtensionMiddleware(
        mcp_asgi, DatasheetTaskExtension(state), discovery
    )

    docs_llms_path = endpoint.path("admin/docs/llms.txt")
    docs_prefix = endpoint.path("admin/docs/")
    mcp_path = endpoint.path("mcp")

    async def protected_asgi(scope, receive, send):
        path = scope.get("path", "")
        if path == docs_llms_path:
            await _plain(send, 200, LLMS_TXT.encode())
        elif path.startswith(docs_prefix):
            doc = SERVER_DOCS.doc(path[len(docs_prefix) :])
            if doc is None:
                await _plain(send, 404, b"unknown server document")
            else:
                await _markdown(send, doc.body)
        elif path == mcp_path or path.startswith(f"{mcp_path}/"):
            await task_app(scope, receive, send)
        else:
            await _plain(send, 404, b"not found")

    protected_stack = InternalAuthMiddleware(
        protected_asgi, verifier, logger.warn
    )

    ready = asyncio.Event()
    root = RootApp(
        health_path=endpoint.path("healthz"),
        ready_path=endpoint.path("readyz"),
        docs_llms_path=docs_llms_path,
        docs_prefix=docs_prefix,
        mcp_path=mcp_path,
        protected_app=protected_stack,
        session_manager=session_manager,
        ready=ready,
    )

    allowed_hosts = public_allowed_hosts(deployment, config.allow_loopback_hosts)
    allowed_hosts.extend(config.allowed_hosts)
    app = HostValidationMiddleware(root, allowed_hosts, logger.warn)

    logger.info(
        "listening",
        address=f"0.0.0.0:{config.port}",
        mcp_path=endpoint.path("mcp"),
        public_url=endpoint.public_url,
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
    await server.serve()
    await state.artifacts.close()
    await tasks.store.close()


def main() -> None:
    asyncio.run(serve(parse_config()))


if __name__ == "__main__":
    main()
