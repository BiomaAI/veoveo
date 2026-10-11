#!/usr/bin/env python3
from __future__ import annotations

import logging
import math
import os
import signal
import select
import threading
import time


def main() -> None:
    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s %(levelname)s %(name)s %(message)s",
    )
    stop_requested = threading.Event()

    def request_stop(_signum: int, _frame: object) -> None:
        stop_requested.set()

    previous = signal.signal(signal.SIGTERM, request_stop)
    try:
        ready = os.environ.pop("VEOVEO_UAV_READY_FD", None)
        acknowledgement = os.environ.pop("VEOVEO_UAV_ACK_FD", None)
        original_deadline = os.environ.pop("VEOVEO_UAV_READY_DEADLINE", None)
        if any(value is not None for value in (ready, acknowledgement, original_deadline)):
            descriptor = None
            ack_descriptor = None
            try:
                try:
                    descriptor = int(ready)
                    ack_descriptor = int(acknowledgement)
                    deadline = float(original_deadline)
                except (TypeError, ValueError):
                    raise RuntimeError("invalid UAV launcher readiness configuration") from None
                if min(descriptor, ack_descriptor) <= 2 or descriptor == ack_descriptor:
                    raise RuntimeError("invalid UAV launcher readiness descriptors")
                if not math.isfinite(deadline) or not 0 < deadline - time.monotonic() <= 10.0:
                    raise RuntimeError("UAV launcher readiness deadline expired or invalid")
                os.write(descriptor, f"{os.getpid()}\n".encode("ascii"))
                os.close(descriptor)
                descriptor = None
                message = bytearray()
                while True:
                    remaining = deadline - time.monotonic()
                    if remaining <= 0 or not select.select([ack_descriptor], [], [], remaining)[0]:
                        raise RuntimeError("UAV launcher acknowledgement timed out")
                    chunk = os.read(ack_descriptor, 2)
                    if not chunk:
                        break
                    message.extend(chunk)
                    if message != b"\x01":
                        raise RuntimeError("invalid UAV launcher acknowledgement")
                if message != b"\x01":
                    raise RuntimeError("missing UAV launcher acknowledgement")
            finally:
                for owned in {descriptor, ack_descriptor}:
                    if owned is not None and owned > 2:
                        os.close(owned)
        if stop_requested.is_set():
            return
        from veoveo_uav_sim import RuntimeConfig
        from veoveo_uav_sim.app import run

        config = RuntimeConfig.from_environment()
        if not stop_requested.is_set():
            run(config, stop_requested)
    finally:
        signal.signal(signal.SIGTERM, previous)


if __name__ == "__main__":
    main()
