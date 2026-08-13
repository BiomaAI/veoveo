from __future__ import annotations

import asyncio
import socket
import threading
import time
import unittest
import urllib.request
from types import SimpleNamespace

from aiohttp import web

from veoveo_uav_sim.adapter_server import AdapterServer


def _available_port() -> int:
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        return int(listener.getsockname()[1])


class AdapterServerTests(unittest.TestCase):
    def test_close_cancels_a_live_stream_handler(self) -> None:
        stream_started = threading.Event()

        async def stream(request: web.Request) -> web.StreamResponse:
            response = web.StreamResponse(status=200)
            await response.prepare(request)
            stream_started.set()
            await asyncio.Event().wait()
            return response

        application = web.Application()
        application.router.add_get("/events", stream)
        port = _available_port()
        config = SimpleNamespace(adapter_host="127.0.0.1", adapter_port=port)
        server = AdapterServer(config, application)
        server.start()

        client_finished = threading.Event()

        def open_stream() -> None:
            try:
                with urllib.request.urlopen(
                    f"http://127.0.0.1:{port}/events", timeout=5.0
                ) as response:
                    response.read()
            except Exception:
                pass
            finally:
                client_finished.set()

        client = threading.Thread(target=open_stream, daemon=True)
        client.start()
        self.assertTrue(stream_started.wait(2.0))

        started = time.monotonic()
        server.close()

        self.assertLess(time.monotonic() - started, 2.0)
        self.assertTrue(client_finished.wait(2.0))


if __name__ == "__main__":
    unittest.main()
