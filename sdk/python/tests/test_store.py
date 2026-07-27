import asyncio

import pytest

from veoveo_mcp.tasks.store import StoreError, SurrealStore


class FakeReceiveTask:
    def __init__(self, done: bool) -> None:
        self._done = done

    def done(self) -> bool:
        return self._done


class FakeConnection:
    def __init__(
        self,
        response: dict | None = None,
        *,
        stale: bool = False,
        query_error: Exception | None = None,
        stale_on_error: bool = False,
    ) -> None:
        self.response = response or {"result": [{"status": "OK", "result": [1]}]}
        self.recv_task = FakeReceiveTask(stale)
        self.socket = object()
        self.query_error = query_error
        self.stale_on_error = stale_on_error
        self.closed = False
        self.queries = 0

    async def query_raw(self, _sql: str, _vars: dict) -> dict:
        self.queries += 1
        if self.query_error is not None:
            if self.stale_on_error:
                self.recv_task = FakeReceiveTask(True)
            raise self.query_error
        return self.response

    async def close(self) -> None:
        self.closed = True


@pytest.mark.asyncio
async def test_query_replaces_known_stale_connection_before_dispatch() -> None:
    stale = FakeConnection(stale=True)
    healthy = FakeConnection()
    opens = 0

    async def reconnect() -> FakeConnection:
        nonlocal opens
        opens += 1
        return healthy

    store = SurrealStore(stale, reconnect)

    assert await store.query("RETURN 1;") == [[1]]
    assert opens == 1
    assert stale.closed
    assert stale.queries == 0
    assert healthy.queries == 1


@pytest.mark.asyncio
async def test_query_restores_connection_without_replaying_ambiguous_request() -> None:
    disconnected = FakeConnection(
        query_error=ConnectionError("connection closed"),
        stale_on_error=True,
    )
    healthy = FakeConnection()
    opens = 0

    async def reconnect() -> FakeConnection:
        nonlocal opens
        opens += 1
        return healthy

    store = SurrealStore(disconnected, reconnect)

    with pytest.raises(StoreError, match="restored for the next request"):
        await store.query("CREATE example CONTENT {};")

    assert opens == 1
    assert disconnected.closed
    assert disconnected.queries == 1
    assert healthy.queries == 0
    assert await store.query("RETURN 1;") == [[1]]
    assert healthy.queries == 1


@pytest.mark.asyncio
async def test_query_propagates_caller_cancellation_without_reconnecting() -> None:
    started = asyncio.Event()

    class BlockingConnection(FakeConnection):
        async def query_raw(self, _sql: str, _vars: dict) -> dict:
            started.set()
            await asyncio.Event().wait()
            raise AssertionError("unreachable")

    connection = BlockingConnection()
    opens = 0

    async def reconnect() -> FakeConnection:
        nonlocal opens
        opens += 1
        return FakeConnection()

    store = SurrealStore(connection, reconnect)
    task = asyncio.create_task(store.query("RETURN 1;"))
    await started.wait()
    task.cancel()

    with pytest.raises(asyncio.CancelledError):
        await task

    assert opens == 0
