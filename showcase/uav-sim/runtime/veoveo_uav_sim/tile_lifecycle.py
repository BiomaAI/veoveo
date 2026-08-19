from __future__ import annotations

import logging
import queue
from dataclasses import dataclass
from typing import Any, Literal, Protocol

LOGGER = logging.getLogger("veoveo.uav_sim.tiles")

TILESET_LOADED_EVENT = "cesium.omniverse.TILESET_LOADED"
TILESET_LOAD_FAILED_EVENT = "cesium.omniverse.TILESET_LOAD_FAILED"

type TileLifecycle = Literal[
    "connecting",
    "streaming",
    "ready",
    "refreshing",
    "degraded",
]
type TileLoadType = Literal[
    "ion_endpoint",
    "tileset_json",
    "tile_content",
    "unknown",
]
type TileFailureCode = Literal[
    "provider_session_rejected",
    "credentials_rejected",
    "asset_unavailable",
    "quota_exceeded",
    "provider_unavailable",
    "transport_failed",
    "request_failed",
]


@dataclass(frozen=True, slots=True)
class NativeTileEvent:
    kind: Literal["loaded", "load_failed"]
    tileset_path: str
    generation: int
    load_type: TileLoadType = "unknown"
    http_status: int = 0


@dataclass(frozen=True, slots=True)
class TileFailure:
    code: TileFailureCode
    load_type: TileLoadType
    http_status: int
    generation: int


@dataclass(frozen=True, slots=True)
class TileLifecycleSnapshot:
    lifecycle: TileLifecycle
    provider_generation: int
    event_sequence: int
    refresh_count: int
    resident_tiles: int
    visible_tiles: int
    loading_tiles: int
    last_failure: TileFailure | None
    diagnostic: str | None


@dataclass(frozen=True, slots=True)
class TileLifecycleAction:
    reset_provider_session: bool = False
    report_failure: bool = False


class CesiumTilesetInterface(Protocol):
    """Native operations required for a fresh provider generation."""

    def clear_accessor_cache(self) -> None: ...

    def reload_tileset(self, tileset_path: str) -> None: ...


def reset_provider_session(
    cesium_interface: CesiumTilesetInterface,
    tileset_path: str,
) -> None:
    """Discard cached provider URLs before constructing the next tileset."""
    cesium_interface.clear_accessor_cache()
    cesium_interface.reload_tileset(tileset_path)


def classify_failure(load_type: TileLoadType, http_status: int) -> TileFailureCode:
    if load_type == "tile_content" and http_status == 400:
        return "provider_session_rejected"
    if http_status in {401, 403}:
        return "credentials_rejected"
    if http_status == 404:
        return "asset_unavailable"
    if http_status == 429:
        return "quota_exceeded"
    if 500 <= http_status <= 599:
        return "provider_unavailable"
    if http_status == 0:
        return "transport_failed"
    return "request_failed"


def failure_diagnostic(code: TileFailureCode) -> str:
    return {
        "provider_session_rejected": ("streamed-world provider session was rejected"),
        "credentials_rejected": "streamed-world credentials were rejected",
        "asset_unavailable": "streamed-world asset is unavailable",
        "quota_exceeded": "streamed-world provider quota was exceeded",
        "provider_unavailable": "streamed-world provider is unavailable",
        "transport_failed": "streamed-world transport failed",
        "request_failed": "streamed-world request failed",
    }[code]


class TileLifecycleController:
    """Reduce native load events and render observations into safe tile state."""

    def __init__(self, *, tileset_path: str, ready_frames: int) -> None:
        if not tileset_path:
            raise ValueError("tileset path must not be empty")
        if ready_frames < 1:
            raise ValueError("ready frame threshold must be positive")
        self._tileset_path = tileset_path
        self._ready_frames = ready_frames
        self._lifecycle: TileLifecycle = "connecting"
        self._provider_generation = 0
        self._event_sequence = 0
        self._refresh_count = 0
        self._coverage_frames = 0
        self._resident_tiles = 0
        self._visible_tiles = 0
        self._loading_tiles = 0
        self._last_failure: TileFailure | None = None
        self._diagnostic: str | None = None
        self._refresh_target_generation: int | None = None
        self._loaded_generation = 0
        self._handled_failures: set[tuple[int, TileLoadType, int]] = set()

    def accept(self, event: NativeTileEvent) -> TileLifecycleAction:
        if event.tileset_path != self._tileset_path:
            return TileLifecycleAction()
        if event.generation < 1:
            LOGGER.warning("ignored Cesium event with invalid generation")
            return TileLifecycleAction()
        if event.generation < self._provider_generation:
            return TileLifecycleAction()
        if event.kind == "loaded" and event.generation <= self._loaded_generation:
            return TileLifecycleAction()

        self._event_sequence += 1
        self._provider_generation = max(self._provider_generation, event.generation)
        if event.kind == "loaded":
            self._loaded_generation = max(self._loaded_generation, event.generation)
            if (
                self._refresh_target_generation is None
                or event.generation >= self._refresh_target_generation
            ):
                self._coverage_frames = 0
                self._lifecycle = "streaming"
                self._diagnostic = None
            return TileLifecycleAction()

        failure_signature = (
            event.generation,
            event.load_type,
            max(0, event.http_status),
        )
        if failure_signature in self._handled_failures:
            return TileLifecycleAction()
        self._handled_failures.add(failure_signature)
        code = classify_failure(event.load_type, event.http_status)
        self._last_failure = TileFailure(
            code=code,
            load_type=event.load_type,
            http_status=max(0, event.http_status),
            generation=event.generation,
        )
        self._diagnostic = failure_diagnostic(code)
        self._coverage_frames = 0

        if (
            code == "provider_session_rejected"
            and self._refresh_target_generation is None
        ):
            self._refresh_count += 1
            self._refresh_target_generation = event.generation + 1
            self._lifecycle = "refreshing"
            return TileLifecycleAction(
                reset_provider_session=True,
                report_failure=True,
            )

        self._lifecycle = "degraded"
        return TileLifecycleAction(report_failure=True)

    def observe_render(
        self,
        *,
        resident_tiles: int,
        visible_tiles: int,
        loading_tiles: int,
    ) -> TileLifecycleSnapshot:
        self._resident_tiles = max(0, resident_tiles)
        self._visible_tiles = max(0, visible_tiles)
        self._loading_tiles = max(0, loading_tiles)

        generation_ready = (
            self._refresh_target_generation is None
            or self._loaded_generation >= self._refresh_target_generation
        )
        if self._visible_tiles > 0 and generation_ready:
            self._coverage_frames += 1
            if self._coverage_frames >= self._ready_frames:
                self._lifecycle = "ready"
                self._diagnostic = None
                self._refresh_target_generation = None
        elif self._lifecycle not in {"refreshing", "degraded"}:
            self._coverage_frames = 0
            self._lifecycle = (
                "streaming"
                if self._resident_tiles > 0 or self._loading_tiles > 0
                else "connecting"
            )
        return self.snapshot()

    def mark_refresh_command_failed(self) -> None:
        self._lifecycle = "degraded"
        self._diagnostic = "streamed-world refresh command failed"

    def snapshot(self) -> TileLifecycleSnapshot:
        return TileLifecycleSnapshot(
            lifecycle=self._lifecycle,
            provider_generation=self._provider_generation,
            event_sequence=self._event_sequence,
            refresh_count=self._refresh_count,
            resident_tiles=self._resident_tiles,
            visible_tiles=self._visible_tiles,
            loading_tiles=self._loading_tiles,
            last_failure=self._last_failure,
            diagnostic=self._diagnostic,
        )


class NativeTileEventBridge:
    """Translate Cesium message-bus payloads into a nonblocking typed queue."""

    def __init__(self) -> None:
        import carb.events
        import omni.kit.app

        self._events: queue.SimpleQueue[NativeTileEvent] = queue.SimpleQueue()
        stream = omni.kit.app.get_app().get_message_bus_event_stream()
        self._subscriptions = [
            stream.create_subscription_to_pop_by_type(
                carb.events.type_from_string(TILESET_LOADED_EVENT),
                self._on_loaded,
                name="veoveo.uav_sim.tiles.loaded",
            ),
            stream.create_subscription_to_pop_by_type(
                carb.events.type_from_string(TILESET_LOAD_FAILED_EVENT),
                self._on_failed,
                name="veoveo.uav_sim.tiles.load_failed",
            ),
        ]

    def drain(self) -> tuple[NativeTileEvent, ...]:
        events: list[NativeTileEvent] = []
        while True:
            try:
                events.append(self._events.get_nowait())
            except queue.Empty:
                return tuple(events)

    def close(self) -> None:
        self._subscriptions.clear()

    def _on_loaded(self, event: Any) -> None:
        parsed = self._parse(event, kind="loaded")
        if parsed is not None:
            self._events.put(parsed)

    def _on_failed(self, event: Any) -> None:
        parsed = self._parse(event, kind="load_failed")
        if parsed is not None:
            self._events.put(parsed)

    @staticmethod
    def _parse(
        event: Any, *, kind: Literal["loaded", "load_failed"]
    ) -> NativeTileEvent | None:
        try:
            payload = event.payload
            path = str(payload["tilesetPath"])
            generation = int(payload["generation"])
            try:
                load_type = str(payload["loadType"])
            except KeyError:
                load_type = "unknown"
            if load_type not in {
                "ion_endpoint",
                "tileset_json",
                "tile_content",
                "unknown",
            }:
                load_type = "unknown"
            return NativeTileEvent(
                kind=kind,
                tileset_path=path,
                generation=generation,
                load_type=load_type,
                http_status=(
                    int(payload["statusCode"]) if kind == "load_failed" else 0
                ),
            )
        except (KeyError, TypeError, ValueError):
            LOGGER.exception("ignored malformed redacted Cesium lifecycle event")
            return None
