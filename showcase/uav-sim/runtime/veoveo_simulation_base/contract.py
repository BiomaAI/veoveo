"""Read the embedded canonical simulation build lock."""

from __future__ import annotations

import hashlib
import json
from pathlib import Path
from typing import Any

BUILD_LOCK_PATH = Path("/opt/veoveo/simulation-base/simulation-runtime.lock.json")
BUILD_LOCK_DIGEST_PATH = Path("/opt/veoveo/simulation-base/simulation-runtime.lock.sha256")


def read_build_lock() -> dict[str, Any]:
    """Load the exact build lock after checking its embedded SHA-256 identity."""

    payload = BUILD_LOCK_PATH.read_bytes()
    actual = hashlib.sha256(payload).hexdigest()
    expected = BUILD_LOCK_DIGEST_PATH.read_text(encoding="utf-8").strip()
    if actual != expected:
        raise RuntimeError(
            f"simulation build lock digest differs: expected {expected}, received {actual}"
        )
    lock = json.loads(payload)
    if lock.get("schemaVersion") != "veoveo.io/simulation-runtime-build-lock/v1":
        raise RuntimeError("simulation build lock has an unsupported schema")
    return lock
