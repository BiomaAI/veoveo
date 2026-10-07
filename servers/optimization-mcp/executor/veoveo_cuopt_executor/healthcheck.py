import asyncio
import json
import os
from pathlib import Path

from . import PROTOCOL_VERSION
from .protocol import ExecutorResponse, validation_diagnostic
from pydantic import ValidationError

MAXIMUM_RESPONSE_BYTES = 1024 * 1024
SOCKET_PATH = Path(
    os.environ.get("VEOVEO_CUOPT_SOCKET", "/run/veoveo-cuopt/executor.sock")
)


async def check() -> None:
    reader, writer = await asyncio.open_unix_connection(SOCKET_PATH)
    try:
        request = json.dumps(
            {
                "protocol": PROTOCOL_VERSION,
                'runId': "run-healthcheck",
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
        try:
            response = ExecutorResponse.model_validate_json(await reader.readexactly(length))
        except ValidationError as error:
            raise RuntimeError(validation_diagnostic(error)) from error
        if response.protocol != PROTOCOL_VERSION or response.runId != "run-healthcheck":
            raise RuntimeError("health response protocol mismatch")
        result = response.result
        if result.result != "health" or not result.health.ready:
            raise RuntimeError("cuOpt executor is not ready")
    finally:
        writer.close()
        await writer.wait_closed()


def main() -> None:
    asyncio.run(asyncio.wait_for(check(), timeout=3))


if __name__ == "__main__":
    main()
