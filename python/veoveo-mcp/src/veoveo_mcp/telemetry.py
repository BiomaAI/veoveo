"""Structured JSON-line logging for hosted Python MCP servers.

Emits one JSON object per line with `message` and `service` fields, matching
what the Rust smoke assertions expect from hosted servers.
"""

from __future__ import annotations

import json
import sys
from datetime import datetime, timezone
from typing import Any, TextIO


class JsonLogger:
    def __init__(self, service: str, stream: TextIO | None = None) -> None:
        self.service = service
        self.stream = stream if stream is not None else sys.stdout

    def log(self, level: str, message: str, **fields: Any) -> None:
        record = {
            "timestamp": datetime.now(timezone.utc).isoformat(),
            "level": level,
            "service": self.service,
            "message": message,
            **fields,
        }
        self.stream.write(json.dumps(record) + "\n")
        self.stream.flush()

    def info(self, message: str, **fields: Any) -> None:
        self.log("INFO", message, **fields)

    def warn(self, message: str, **fields: Any) -> None:
        self.log("WARN", message, **fields)

    def error(self, message: str, **fields: Any) -> None:
        self.log("ERROR", message, **fields)
