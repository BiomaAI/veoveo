from __future__ import annotations

import asyncio
import json
import logging
import threading
from dataclasses import dataclass

from aiohttp import web


LOGGER = logging.getLogger(__name__)
RUNTIME_EVENT_SCHEMA = "veoveo.io/uav-runtime-event/v2"


@dataclass(frozen=True, slots=True)
class RuntimeEvent:
    event: str
    session_id: str
    generation: int

    def encode(self) -> bytes:
        return json.dumps(
            {
                "schema": RUNTIME_EVENT_SCHEMA,
                "event": self.event,
                "sessionId": self.session_id,
                "generation": self.generation,
            },
            separators=(",", ":"),
            sort_keys=True,
        ).encode("utf-8")


class RuntimeEventPublisher:
    """Publishes lifecycle edges to authenticated long-lived HTTP subscribers."""

    def __init__(self) -> None:
        self._lock = threading.Lock()
        self._latest: RuntimeEvent | None = None
        self._subscribers: dict[
            int, tuple[asyncio.AbstractEventLoop, asyncio.Queue[bytes]]
        ] = {}
        self._next_subscriber_id = 1

    def publish(
        self, *, event: str, session_id: str, generation: int
    ) -> None:
        if event not in {"adapter_ready", "ready"}:
            raise ValueError("runtime event kind is not supported")
        if not session_id or generation < 1:
            raise ValueError("runtime event identity is invalid")
        runtime_event = RuntimeEvent(event, session_id, generation)
        payload = runtime_event.encode()
        with self._lock:
            self._latest = runtime_event
            subscribers = tuple(self._subscribers.values())
        for loop, queue in subscribers:
            loop.call_soon_threadsafe(self._offer, queue, payload)

    async def stream(self, request: web.Request) -> web.StreamResponse:
        response = web.StreamResponse(
            status=200,
            headers={
                "Content-Type": "application/x-ndjson",
                "Cache-Control": "no-store",
                "X-Content-Type-Options": "nosniff",
            },
        )
        await response.prepare(request)
        loop = asyncio.get_running_loop()
        queue: asyncio.Queue[bytes] = asyncio.Queue(maxsize=1)
        with self._lock:
            subscriber_id = self._next_subscriber_id
            self._next_subscriber_id += 1
            self._subscribers[subscriber_id] = (loop, queue)
            latest = self._latest
        if latest is not None:
            self._offer(queue, latest.encode())
        try:
            while True:
                payload = await queue.get()
                await response.write(payload + b"\n")
        except (asyncio.CancelledError, ConnectionError):
            raise
        finally:
            with self._lock:
                self._subscribers.pop(subscriber_id, None)
        return response

    @staticmethod
    def _offer(queue: asyncio.Queue[bytes], payload: bytes) -> None:
        if queue.full():
            try:
                queue.get_nowait()
            except asyncio.QueueEmpty:
                pass
        queue.put_nowait(payload)


def notify_adapter_ready(
    publisher: RuntimeEventPublisher,
    *,
    session_id: str,
    generation: int,
) -> None:
    publisher.publish(
        event="adapter_ready",
        session_id=session_id,
        generation=generation,
    )


def notify_runtime_ready(
    publisher: RuntimeEventPublisher,
    *,
    session_id: str,
    generation: int,
) -> None:
    publisher.publish(
        event="ready",
        session_id=session_id,
        generation=generation,
    )
