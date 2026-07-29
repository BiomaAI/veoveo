import asyncio
import json
import os
from pathlib import Path

from . import PROTOCOL_VERSION

MAXIMUM_RESPONSE_BYTES = 1024 * 1024
SOCKET_PATH = Path(
    os.environ.get("VEOVEO_CUOPT_SOCKET", "/run/veoveo-cuopt/executor.sock")
)


async def check() -> None:
    reader, writer = await asyncio.open_unix_connection(SOCKET_PATH)
    request = json.dumps(
        {
            "protocol": PROTOCOL_VERSION,
            "run_id": "run-healthcheck",
            "operation": {"operation": "health"},
        },
        separators=(",", ":"),
    ).encode("utf-8")
    writer.write(len(request).to_bytes(8, byteorder="big", signed=False))
    writer.write(request)
    await writer.drain()
    length = int.from_bytes(await reader.readexactly(8), byteorder="big")
    if length > MAXIMUM_RESPONSE_BYTES:
        raise RuntimeError("health response exceeds the probe limit")
    response = json.loads(await reader.readexactly(length))
    writer.close()
    await writer.wait_closed()
    if response.get("protocol") != PROTOCOL_VERSION:
        raise RuntimeError("health response protocol mismatch")
    result = response.get("result", {})
    if result.get("result") != "health" or not result.get("health", {}).get("ready"):
        raise RuntimeError("cuOpt executor is not ready")


def main() -> None:
    asyncio.run(asyncio.wait_for(check(), timeout=3))


if __name__ == "__main__":
    main()
