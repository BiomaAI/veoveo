"""Persistent CUDA inference; only a mode-0600 Unix socket is exposed."""
from __future__ import annotations

import argparse
import asyncio
import contextlib
import os
from pathlib import Path
import struct

from .protocol import MAX_FRAME_BYTES, MODEL, PROTOCOL, REVISION, Request, encode, transcript


class Worker:
    def __init__(self, model, work: Path, device: str, capacity: int):
        self.model = model
        self.work = work
        self.device = device
        self.capacity = capacity
        self.active = 0

    async def emit(self, writer: asyncio.StreamWriter, value: dict[str, object]):
        writer.write(encode(value))
        await asyncio.wait_for(writer.drain(), 10)

    async def handle(self, reader: asyncio.StreamReader, writer: asyncio.StreamWriter):
        admitted = False
        try:
            line = await asyncio.wait_for(reader.readline(), 5)
            if not line.endswith(b"\n"):
                raise ValueError("incomplete request")
            request = Request.parse(line, self.work)
            if request.operation == "probe":
                await self.emit(writer, {"kind": "ready", "protocol": PROTOCOL,
                    "device": self.device, "model": MODEL, "revision": REVISION})
                return
            if self.active >= self.capacity:
                await self.emit(writer, {"kind": "error", "code": "capacity"})
                return
            self.active += 1
            admitted = True
            await self.emit(writer, {"kind": "accepted"})
            # Deadline bounds a client that keeps a connection open without finishing.
            timeout = request.max_duration_seconds + 30 if request.operation == "live" else 600
            async with asyncio.timeout(timeout):
                await self.run(request, reader, writer)
        except (ValueError, KeyError, TypeError, OSError):
            await self.error(writer, "invalid_input")
        except TimeoutError:
            await self.error(writer, "timed_out")
        except (ConnectionError, asyncio.IncompleteReadError):
            pass
        except Exception:
            # Provider diagnostics may include local paths or audio-derived content.
            await self.error(writer, "inference_failed")
        finally:
            if admitted:
                self.active -= 1
            writer.close()
            with contextlib.suppress(ConnectionError, OSError):
                await writer.wait_closed()

    async def error(self, writer, code):
        with contextlib.suppress(ConnectionError, OSError, TimeoutError):
            await self.emit(writer, {"kind": "error", "code": code})

    async def run(self, request, reader, writer):
        if request.operation == "file":
            # Native metadata inspection rejects excessive duration before inference.
            import kestrel_native
            with contextlib.closing(kestrel_native.open_audio_file_mono(
                    request.path, max_duration_seconds=request.max_duration_seconds)):
                pass
            audio = request.path
            options = {}
            producer = asyncio.create_task(reader.read(1))
        else:
            queue = asyncio.Queue(maxsize=8)
            producer = asyncio.create_task(self.pump(reader, queue, request))

            async def chunks():
                while (chunk := await queue.get()) is not None:
                    yield chunk

            audio = chunks()
            options = {"sample_rate": request.sample_rate}

        async def infer():
            updates = await self.model.atranscribe(audio=audio, timestamps="word", stream=True, **options)
            try:
                async for update in updates:
                    await self.emit(writer, {"kind": "transcript", "complete": False,
                        "transcript": transcript(update, request.max_duration_seconds)})
                result = await updates.aresult()
                await self.emit(writer, {"kind": "transcript", "complete": True,
                    "transcript": transcript(result, request.max_duration_seconds)})
            finally:
                await updates.aclose()

        inference = asyncio.create_task(infer())
        try:
            done, _ = await asyncio.wait((producer, inference), return_when=asyncio.FIRST_COMPLETED)
            if inference in done:
                await inference
            else:
                await producer
                raise ConnectionError("peer disconnected")
        finally:
            producer.cancel()
            inference.cancel()
            await asyncio.gather(producer, inference, return_exceptions=True)

    async def pump(self, reader, queue, request):
        import numpy as np
        samples = 0
        while True:
            prefix = await asyncio.wait_for(reader.readexactly(4), 10)
            size = struct.unpack(">I", prefix)[0]
            if size == 0:
                await queue.put(None)
                # Keep observing disconnect after input completion, until the final output.
                await reader.read(1)
                return
            if size > MAX_FRAME_BYTES or size % 4:
                raise ValueError("invalid PCM frame")
            data = await asyncio.wait_for(reader.readexactly(size), 10)
            chunk = np.frombuffer(data, dtype="<f4")
            samples += chunk.size
            if samples > request.sample_rate * request.max_duration_seconds:
                raise ValueError("live duration exceeded")
            if not np.isfinite(chunk).all() or np.max(np.abs(chunk)) > 1.0:
                raise ValueError("invalid PCM samples")
            await queue.put(chunk)


async def serve(args):
    import numpy as np
    import torch
    import moondream as md
    from kestrel.models.parakeet_tdt.weights import ULTRA_REVISION
    if not torch.cuda.is_available() or ULTRA_REVISION != REVISION:
        raise RuntimeError("qualified CUDA runtime and model revision required")
    work = args.work_dir.resolve(strict=True)
    socket = args.socket.absolute()
    if socket.exists() or not socket.parent.resolve().is_relative_to(work):
        raise ValueError("socket must be new and inside the private workspace")
    os.umask(0o077)
    with md.photon(MODEL, device="cuda", single_pass_batch_capacity=args.capacity) as model:
        # Warm up actual acoustic inference before opening the readiness boundary.
        model.transcribe(audio=np.zeros(1600, dtype=np.float32), sample_rate=16000)
        torch.cuda.synchronize()
        if torch.cuda.memory_allocated() < 500_000_000:
            raise RuntimeError("CUDA model residency was not established")
        worker = Worker(model, work, "cuda:" + torch.cuda.get_device_name(0), args.capacity)
        server = await asyncio.start_unix_server(worker.handle, socket, limit=16_384)
        os.chmod(socket, 0o600)
        try:
            async with server:
                await server.serve_forever()
        finally:
            socket.unlink(missing_ok=True)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--socket", type=Path, required=True)
    parser.add_argument("--work-dir", type=Path, required=True)
    parser.add_argument("--capacity", type=int, choices=range(1, 9), default=4)
    args = parser.parse_args()
    try:
        asyncio.run(serve(args))
    except KeyboardInterrupt:
        pass


if __name__ == "__main__":
    main()
