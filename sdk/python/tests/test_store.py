import asyncio

import pytest

from query_files import test_query
from veoveo_mcp.tasks import InvalidRecord
from veoveo_mcp.tasks.store import StoreError, SurrealStore, task_result_from_store


@pytest.mark.parametrize(
    "value", [{}, {"value": 42}, {"payload": None, "extra": True}, [], 42]
)
def test_result_decoder_rejects_noncanonical_envelopes(value):
    with pytest.raises(InvalidRecord, match="invalid stored Task result envelope"):
        task_result_from_store(value)


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

    assert await store.query(test_query("test_store/test_query_replaces_known_stale_connection_before_dispatch.surql")) == [[1]]
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
        await store.query(test_query("test_store/test_query_restores_connection_without_replaying_ambiguous_request.surql"))

    assert opens == 1
    assert disconnected.closed
    assert disconnected.queries == 1
    assert healthy.queries == 0
    assert await store.query(test_query("test_store/test_query_restores_connection_without_replaying_ambiguous_request_2.surql")) == [[1]]
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
    task = asyncio.create_task(store.query(test_query("test_store/test_query_propagates_caller_cancellation_without_reconnecting.surql")))
    await started.wait()
    task.cancel()

    with pytest.raises(asyncio.CancelledError):
        await task

    assert opens == 0


@pytest.mark.parametrize("cid_state", ["owned", "missing", "malformed"])
def test_fixture_cleans_owned_container_after_launch_timeout(monkeypatch, tmp_path, cid_state):
    import json
    import subprocess
    from pathlib import Path

    import conftest

    calls = []
    owned_cid = "a" * 64
    directory = tmp_path / "fixture"
    directory.mkdir()
    monkeypatch.setattr(conftest, "_docker_available", lambda: True)
    monkeypatch.setattr(conftest, "_gateway_binary", lambda: tmp_path / "gateway")
    monkeypatch.setattr(conftest, "_free_port", lambda: 12345)
    monkeypatch.setattr(conftest.tempfile, "mkdtemp", lambda **_options: str(directory))

    def run(arguments, **options):
        calls.append((arguments, options))
        if arguments[:2] == ["docker", "run"]:
            cidfile = Path(arguments[arguments.index("--cidfile") + 1])
            if cid_state != "missing":
                cidfile.write_text(owned_cid if cid_state == "owned" else "invalid-cid")
            raise subprocess.TimeoutExpired(
                arguments, options["timeout"], stderr=conftest.RUNTIME_PASSWORD,
            )
        assert arguments == ["docker", "rm", "--force", "--volumes", owned_cid]
        return subprocess.CompletedProcess(arguments, 0, stdout=owned_cid + "\n", stderr="")

    monkeypatch.setattr(conftest.subprocess, "run", run)
    fixture = conftest.surreal_platform.__wrapped__()
    expected = "launch exceeded 60 seconds" if cid_state == "owned" else "cleanup unresolved"
    with pytest.raises(RuntimeError, match=expected) as error:
        next(fixture)

    receipt = json.loads((directory / "receipt.json").read_text())
    assert receipt["launch"] == "unknown"
    assert conftest.RUNTIME_PASSWORD not in json.dumps(receipt)
    assert conftest.RUNTIME_PASSWORD not in str(error.value)
    assert receipt["diagnostic"] == "[REDACTED]"
    assert calls[0][0][:2] == ["docker", "run"]
    assert calls[0][1]["timeout"] == 60
    assert receipt["name"] == calls[0][0][calls[0][0].index("--name") + 1]
    assert (directory.stat().st_mode & 0o777) == 0o700
    assert ((directory / "receipt.json").stat().st_mode & 0o777) == 0o600
    if cid_state == "owned":
        assert len(calls) == 2
        assert receipt["cid"] == owned_cid
        assert receipt["cleanup"] == "settled"
        assert calls[1][0] == ["docker", "rm", "--force", "--volumes", owned_cid]
        assert calls[1][1] == {
            "check": False, "capture_output": True, "text": True, "timeout": 30,
        }
    else:
        assert len(calls) == 1, "unknown launch identity cannot authorize cleanup"
        assert "cid" not in receipt
        assert receipt["cleanup"] == "unresolved"
        assert "cleanupDiagnostic" in receipt
