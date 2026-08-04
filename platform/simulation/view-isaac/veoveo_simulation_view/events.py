from __future__ import annotations

import logging
import threading
import time
import urllib.error
import urllib.request

from .config import RendererConfig

LOGGER = logging.getLogger("veoveo.simulation_view.events")


def announce_runtime_generation(config: RendererConfig, generation: str) -> None:
    thread = threading.Thread(
        target=_deliver,
        args=(config.runtime_event_url, generation, config.control_token),
        name="simulation-view-runtime-event",
        daemon=True,
    )
    thread.start()


def _deliver(base_url: str, generation: str, token: str) -> None:
    delay_seconds = 0.25
    request = urllib.request.Request(
        f"{base_url}/{generation}",
        method="POST",
        headers={"Authorization": f"Bearer {token}", "Content-Length": "0"},
    )
    while True:
        try:
            with urllib.request.urlopen(request, timeout=5.0) as response:
                if response.status == 204:
                    LOGGER.info(
                        "renderer runtime generation announced: generation=%s",
                        generation,
                    )
                    return
                if response.status < 500:
                    LOGGER.error(
                        "renderer runtime generation was rejected: status=%s",
                        response.status,
                    )
                    return
        except urllib.error.HTTPError as error:
            if error.code < 500:
                LOGGER.error(
                    "renderer runtime generation was rejected: status=%s",
                    error.code,
                )
                return
        except OSError:
            pass
        time.sleep(delay_seconds)
        delay_seconds = min(delay_seconds * 2.0, 30.0)
