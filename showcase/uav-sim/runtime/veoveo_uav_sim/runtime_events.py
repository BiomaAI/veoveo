from __future__ import annotations

import json
import logging
import socket
from pathlib import Path


LOGGER = logging.getLogger(__name__)
RUNTIME_EVENT_SCHEMA = "veoveo.io/uav-runtime-event/v1"


def notify_runtime_ready(
    socket_path: Path,
    *,
    session_id: str,
    generation: int,
) -> bool:
    """Send one pod-local readiness edge without waiting for a consumer."""
    if not session_id or generation < 1:
        LOGGER.error("UAV runtime readiness notification has an invalid identity")
        return False
    payload = json.dumps(
        {
            "schema": RUNTIME_EVENT_SCHEMA,
            "event": "ready",
            "sessionId": session_id,
            "generation": generation,
        },
        separators=(",", ":"),
        sort_keys=True,
    ).encode("utf-8")
    try:
        with socket.socket(socket.AF_UNIX, socket.SOCK_DGRAM) as sender:
            sender.setblocking(False)
            sender.sendto(payload, str(socket_path))
        return True
    except OSError as error:
        LOGGER.info(
            "UAV runtime readiness notification was not delivered; companion startup "
            "will read current state: %s",
            error,
        )
        return False
