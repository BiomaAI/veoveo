from __future__ import annotations

import asyncio
import threading
from typing import Protocol

from aiohttp import web


class AdapterServerConfig(Protocol):
    adapter_host: str
    adapter_port: int


class AdapterServer:
    """Own one aiohttp application and its dedicated event-loop thread."""

    def __init__(
        self, config: AdapterServerConfig, application: web.Application
    ) -> None:
        self._config = config
        self._application = application
        self._thread: threading.Thread | None = None
        self._loop: asyncio.AbstractEventLoop | None = None
        self._runner: web.AppRunner | None = None
        self._started = threading.Event()
        self._error: BaseException | None = None

    def start(self) -> None:
        self._thread = threading.Thread(
            target=self._run, name="uav-adapter-http", daemon=True
        )
        self._thread.start()
        if not self._started.wait(30.0):
            raise TimeoutError("UAV adapter HTTP server did not start")
        if self._error is not None:
            raise RuntimeError("UAV adapter HTTP server failed") from self._error

    def close(self) -> None:
        if self._loop is not None and self._runner is not None:
            future = asyncio.run_coroutine_threadsafe(
                self._runner.cleanup(), self._loop
            )
            try:
                future.result(timeout=5.0)
            finally:
                self._loop.call_soon_threadsafe(self._loop.stop)
        if self._thread is not None:
            self._thread.join(timeout=5.0)
            if self._thread.is_alive():
                raise TimeoutError("UAV adapter HTTP server did not stop")

    def _run(self) -> None:
        try:
            self._loop = asyncio.new_event_loop()
            asyncio.set_event_loop(self._loop)
            # The preconfiguration application is replaced after the immutable
            # world arrives. Its authenticated event feed can have a live
            # subscriber at that boundary, so shutdown must cancel long-lived
            # handlers immediately instead of waiting for the default timeout.
            self._runner = web.AppRunner(
                self._application,
                access_log=None,
                shutdown_timeout=0.1,
            )
            self._loop.run_until_complete(self._runner.setup())
            site = web.TCPSite(
                self._runner, self._config.adapter_host, self._config.adapter_port
            )
            self._loop.run_until_complete(site.start())
            self._started.set()
            self._loop.run_forever()
        except BaseException as error:
            self._error = error
            self._started.set()
