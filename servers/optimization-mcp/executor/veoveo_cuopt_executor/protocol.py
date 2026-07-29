import asyncio
import json
import math
from typing import Any

from . import PROTOCOL_VERSION


class ProtocolError(ValueError):
    pass


def require_mapping(value: Any, label: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise ProtocolError(f"{label} must be an object")
    return value


def require_protocol(request: dict[str, Any]) -> None:
    if request.get("protocol") != PROTOCOL_VERSION:
        raise ProtocolError(
            f"protocol must be exactly {PROTOCOL_VERSION}"
        )
    run_id = request.get("run_id")
    if not isinstance(run_id, str) or not run_id.startswith("run-"):
        raise ProtocolError("run_id must be a controlled run identifier")
    operation = require_mapping(request.get("operation"), "operation")
    if not isinstance(operation.get("operation"), str):
        raise ProtocolError("operation.operation must be a string")


async def read_frame(
    reader: asyncio.StreamReader, maximum_bytes: int
) -> dict[str, Any]:
    prefix = await reader.readexactly(8)
    length = int.from_bytes(prefix, byteorder="big", signed=False)
    if length > maximum_bytes:
        raise ProtocolError(
            f"request frame is {length} bytes and exceeds {maximum_bytes}"
        )
    body = await reader.readexactly(length)
    try:
        request = json.loads(body)
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise ProtocolError(f"request is not valid UTF-8 JSON: {error}") from error
    request = require_mapping(request, "request")
    require_protocol(request)
    return request


async def write_frame(
    writer: asyncio.StreamWriter,
    response: dict[str, Any],
    maximum_bytes: int,
) -> None:
    body = json.dumps(
        response,
        allow_nan=False,
        check_circular=True,
        separators=(",", ":"),
    ).encode("utf-8")
    if len(body) > maximum_bytes:
        raise ProtocolError(
            f"response frame is {len(body)} bytes and exceeds {maximum_bytes}"
        )
    writer.write(len(body).to_bytes(8, byteorder="big", signed=False))
    writer.write(body)
    await writer.drain()


def response(run_id: str, result: dict[str, Any]) -> dict[str, Any]:
    return {
        "protocol": PROTOCOL_VERSION,
        "run_id": run_id,
        "result": result,
    }


def error_response(
    run_id: str, code: str, message: str
) -> dict[str, Any]:
    return response(
        run_id,
        {
            "result": "error",
            "error": {
                "code": code,
                "message": message,
                "findings": [],
            },
        },
    )


def finite_or_none(value: Any) -> float | None:
    if value is None:
        return None
    value = float(value)
    return value if math.isfinite(value) else None
