import asyncio

import pytest

from veoveo_mcp.tasks.store import OutboxWake, StoreError


class LiveFixture:
    def __init__(self):
        self.queue = asyncio.Queue()
        self.started = asyncio.Event()
        self.closed = asyncio.Event()
        self.delivered = []
        self.killed = []

    async def changes(self):
        self.started.set()
        try:
            while True:
                event = await self.queue.get()
                self.delivered.append(event)
                yield event
        finally:
            self.closed.set()

    async def kill(self, live_id):
        self.killed.append(live_id)


@pytest.mark.asyncio
async def test_idle_deadlines_preserve_later_live_delivery():
    fixture = LiveFixture()
    wake = OutboxWake(fixture, "owned-live", fixture.changes())
    try:
        for _ in range(3):
            await wake.wait(0.01)
            assert not fixture.closed.is_set()
        for event in ["first", "second"]:
            fixture.queue.put_nowait(event)
            await wake.wait(0.5)
        assert fixture.delivered == ["first", "second"]
    finally:
        await asyncio.wait_for(wake.close(), 1)
    assert fixture.killed == ["owned-live"]


@pytest.mark.asyncio
async def test_close_releases_an_idle_live_reader():
    fixture = LiveFixture()
    wake = OutboxWake(fixture, "owned-live", fixture.changes())
    await wake.wait(0.01)
    await asyncio.wait_for(wake.close(), 1)
    await asyncio.wait_for(fixture.closed.wait(), 1)
    assert fixture.killed == ["owned-live"]


@pytest.mark.asyncio
async def test_close_releases_a_reader_after_delivery_and_is_idempotent():
    fixture = LiveFixture()
    wake = OutboxWake(fixture, "owned-live", fixture.changes())
    fixture.queue.put_nowait("event")
    await wake.wait(0.5)
    await asyncio.wait_for(wake.close(), 1)
    await asyncio.wait_for(fixture.closed.wait(), 1)
    await asyncio.wait_for(wake.close(), 1)
    assert fixture.killed == ["owned-live"]
    with pytest.raises(StoreError, match="closed"):
        await wake.wait(0.01)


@pytest.mark.asyncio
async def test_finished_live_source_requires_subscription_renewal():
    fixture = LiveFixture()

    async def finished():
        if False:
            yield None

    wake = OutboxWake(fixture, "owned-live", finished())
    try:
        with pytest.raises(StoreError, match="LIVE.*ended") as error:
            await wake.wait(0.1)
        assert error.value.retryable
    finally:
        await asyncio.wait_for(wake.close(), 1)


@pytest.mark.asyncio
async def test_cancelled_consumer_closes_the_owned_live_reader():
    fixture = LiveFixture()
    wake = OutboxWake(fixture, "owned-live", fixture.changes())

    async def consume():
        try:
            await wake.wait(10)
        finally:
            await wake.close()

    task = asyncio.create_task(consume())
    await asyncio.wait_for(fixture.started.wait(), 1)
    task.cancel()
    with pytest.raises(asyncio.CancelledError):
        await asyncio.wait_for(task, 1)
    await asyncio.wait_for(fixture.closed.wait(), 1)
    assert fixture.killed == ["owned-live"]


@pytest.mark.asyncio
@pytest.mark.parametrize("failure", [
    ConnectionError("fixture connection closed"),
    TimeoutError("fixture transport deadline"),
    RuntimeError("fixture driver failure"),
])
async def test_live_transport_failure_propagates(failure):
    fixture = LiveFixture()

    async def failed():
        raise failure
        yield None

    wake = OutboxWake(fixture, "owned-live", failed())
    try:
        with pytest.raises(type(failure)) as error:
            await wake.wait(0.1)
        assert error.value is failure
    finally:
        await asyncio.wait_for(wake.close(), 1)
