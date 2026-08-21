from __future__ import annotations

import logging
import queue
from collections.abc import Callable
from dataclasses import dataclass
from typing import Any, Literal

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
    geometries_loaded: int
    geometries_rendered: int
    materials_loaded: int
    last_failure: TileFailure | None
    diagnostic: str | None


@dataclass(frozen=True, slots=True)
class TileLifecycleAction:
    begin_replacement: bool = False
    retire_tileset_path: str | None = None
    report_failure: bool = False
    retained_textured_coverage: bool = False


@dataclass(frozen=True, slots=True)
class TileRenderStatistics:
    resident_tiles: int
    visible_tiles: int
    loading_tiles: int
    geometries_loaded: int
    geometries_rendered: int
    materials_loaded: int


@dataclass(frozen=True, slots=True)
class TileRenderObservation:
    snapshot: TileLifecycleSnapshot
    action: TileLifecycleAction


def begin_provider_session_replacement(
    author_tileset: Callable[[str], str],
    replacement_name: str,
) -> str:
    """Author a shadow tileset without invalidating resident or disk cache."""
    replacement_path = author_tileset(replacement_name)
    if not replacement_path:
        raise RuntimeError("replacement tileset path must not be empty")
    return replacement_path


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
    if http_status in {0, 408}:
        return "transport_failed"
    return "request_failed"


def has_textured_coverage(
    *,
    visible_tiles: int,
    geometries_rendered: int,
    materials_loaded: int,
) -> bool:
    return (
        visible_tiles > 0
        and geometries_rendered > 0
        and materials_loaded > 0
    )


def tile_content_ready(
    *,
    lifecycle: TileLifecycle,
    visible_tiles: int,
    geometries_rendered: int,
    materials_loaded: int,
) -> bool:
    return lifecycle in {"ready", "refreshing"} and has_textured_coverage(
        visible_tiles=visible_tiles,
        geometries_rendered=geometries_rendered,
        materials_loaded=materials_loaded,
    )


def _is_recoverable_content_failure(
    *,
    code: TileFailureCode,
    load_type: TileLoadType,
) -> bool:
    return load_type == "tile_content" and code in {
        "provider_unavailable",
        "transport_failed",
    }


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

    def __init__(
        self,
        *,
        tileset_path: str,
        ready_frames: int,
        replacement_timeout_frames: int,
    ) -> None:
        if not tileset_path:
            raise ValueError("tileset path must not be empty")
        if ready_frames < 1:
            raise ValueError("ready frame threshold must be positive")
        if replacement_timeout_frames < ready_frames:
            raise ValueError("replacement timeout must cover the readiness window")
        self._tileset_path = tileset_path
        self._replacement_tileset_path: str | None = None
        self._replacement_loaded_generation: int | None = None
        self._replacement_baseline: TileRenderStatistics | None = None
        self._replacement_frames = 0
        self._ready_frames = ready_frames
        self._replacement_timeout_frames = replacement_timeout_frames
        self._lifecycle: TileLifecycle = "connecting"
        self._provider_generation = 0
        self._event_sequence = 0
        self._refresh_count = 0
        self._coverage_frames = 0
        self._resident_tiles = 0
        self._visible_tiles = 0
        self._loading_tiles = 0
        self._geometries_loaded = 0
        self._geometries_rendered = 0
        self._materials_loaded = 0
        self._last_failure: TileFailure | None = None
        self._diagnostic: str | None = None
        self._loaded_generation = 0
        self._recoverable_degradation = False
        self._handled_failures: set[tuple[str, int, TileLoadType, int]] = set()

    @property
    def active_tileset_path(self) -> str:
        return self._tileset_path

    @property
    def replacement_tileset_path(self) -> str | None:
        return self._replacement_tileset_path

    def replacement_started(self, tileset_path: str) -> None:
        if self._lifecycle != "refreshing":
            raise RuntimeError("provider replacement was not requested")
        if self._replacement_tileset_path is not None:
            raise RuntimeError("provider replacement is already active")
        if not tileset_path or tileset_path == self._tileset_path:
            raise ValueError("replacement tileset must have a distinct path")
        self._replacement_tileset_path = tileset_path
        self._replacement_loaded_generation = None
        self._replacement_baseline = TileRenderStatistics(
            resident_tiles=self._resident_tiles,
            visible_tiles=self._visible_tiles,
            loading_tiles=self._loading_tiles,
            geometries_loaded=self._geometries_loaded,
            geometries_rendered=self._geometries_rendered,
            materials_loaded=self._materials_loaded,
        )
        self._replacement_frames = 0

    def accept(self, event: NativeTileEvent) -> TileLifecycleAction:
        if event.tileset_path not in {
            self._tileset_path,
            self._replacement_tileset_path,
        }:
            return TileLifecycleAction()
        if event.generation < 1:
            LOGGER.warning("ignored Cesium event with invalid generation")
            return TileLifecycleAction()
        if (
            event.kind == "loaded"
            and event.tileset_path == self._tileset_path
            and event.generation <= self._loaded_generation
        ):
            return TileLifecycleAction()

        self._event_sequence += 1
        if event.kind == "loaded":
            if event.tileset_path == self._replacement_tileset_path:
                self._replacement_loaded_generation = event.generation
                self._coverage_frames = 0
                self._lifecycle = "refreshing"
                self._diagnostic = (
                    "replacement native generation is awaiting textured coverage"
                )
                return TileLifecycleAction()

            self._provider_generation = max(self._provider_generation, event.generation)
            self._loaded_generation = max(self._loaded_generation, event.generation)
            if self._replacement_tileset_path is None:
                self._recoverable_degradation = False
                self._coverage_frames = 0
                self._lifecycle = "streaming"
                self._diagnostic = None
            return TileLifecycleAction()

        failure_signature = (
            event.tileset_path,
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

        if event.tileset_path == self._replacement_tileset_path:
            failed_path = self._replacement_tileset_path
            self._clear_replacement()
            self._recoverable_degradation = False
            self._lifecycle = "degraded"
            self._diagnostic = "replacement streamed-world provider session failed"
            return TileLifecycleAction(
                retire_tileset_path=failed_path,
                report_failure=True,
            )

        if (
            code == "provider_session_rejected"
            and self._replacement_tileset_path is None
        ):
            self._recoverable_degradation = False
            self._coverage_frames = 0
            self._refresh_count += 1
            self._lifecycle = "refreshing"
            return TileLifecycleAction(
                begin_replacement=True,
                report_failure=True,
            )

        if self._replacement_tileset_path is not None:
            self._lifecycle = "refreshing"
            return TileLifecycleAction(report_failure=True)

        if _is_recoverable_content_failure(code=code, load_type=event.load_type):
            self._recoverable_degradation = True
            self._coverage_frames = 0
            if self._has_textured_coverage():
                self._lifecycle = "ready"
                self._diagnostic = None
                return TileLifecycleAction(
                    report_failure=True,
                    retained_textured_coverage=True,
                )
            self._lifecycle = "degraded"
            return TileLifecycleAction(report_failure=True)

        self._recoverable_degradation = False
        self._coverage_frames = 0
        self._lifecycle = "degraded"
        return TileLifecycleAction(report_failure=True)

    def observe_render(self, statistics: TileRenderStatistics) -> TileRenderObservation:
        self._resident_tiles = max(0, statistics.resident_tiles)
        self._visible_tiles = max(0, statistics.visible_tiles)
        self._loading_tiles = max(0, statistics.loading_tiles)
        self._geometries_loaded = max(0, statistics.geometries_loaded)
        self._geometries_rendered = max(0, statistics.geometries_rendered)
        self._materials_loaded = max(0, statistics.materials_loaded)

        if self._replacement_tileset_path is not None:
            self._replacement_frames += 1
            baseline = self._replacement_baseline
            replacement_has_textured_coverage = (
                self._replacement_loaded_generation is not None
                and baseline is not None
                and self._resident_tiles > baseline.resident_tiles
                and self._geometries_loaded > baseline.geometries_loaded
                and self._materials_loaded > baseline.materials_loaded
                and self._visible_tiles > 0
                and self._geometries_rendered > 0
            )
            if replacement_has_textured_coverage:
                self._coverage_frames += 1
                if self._coverage_frames >= self._ready_frames:
                    retired_path = self._tileset_path
                    self._tileset_path = self._replacement_tileset_path
                    loaded_generation = self._replacement_loaded_generation
                    self._clear_replacement()
                    self._provider_generation = max(1, self._provider_generation + 1)
                    self._loaded_generation = loaded_generation or 1
                    self._recoverable_degradation = False
                    self._lifecycle = "ready"
                    self._diagnostic = None
                    return TileRenderObservation(
                        snapshot=self.snapshot(),
                        action=TileLifecycleAction(retire_tileset_path=retired_path),
                    )
            else:
                self._coverage_frames = 0

            if self._replacement_frames >= self._replacement_timeout_frames:
                failed_path = self._replacement_tileset_path
                self._clear_replacement()
                self._recoverable_degradation = False
                self._lifecycle = "degraded"
                self._diagnostic = (
                    "replacement native generation did not prove textured coverage"
                )
                return TileRenderObservation(
                    snapshot=self.snapshot(),
                    action=TileLifecycleAction(
                        retire_tileset_path=failed_path,
                        report_failure=True,
                    ),
                )

            self._lifecycle = "refreshing"
            return TileRenderObservation(
                snapshot=self.snapshot(), action=TileLifecycleAction()
            )

        textured_coverage = self._has_textured_coverage()
        if self._recoverable_degradation:
            if textured_coverage:
                self._coverage_frames += 1
                if self._coverage_frames >= self._ready_frames:
                    self._recoverable_degradation = False
                    self._lifecycle = "ready"
                    self._diagnostic = None
            else:
                self._coverage_frames = 0
                self._lifecycle = "degraded"
            return TileRenderObservation(
                snapshot=self.snapshot(), action=TileLifecycleAction()
            )

        if self._lifecycle != "degraded" and textured_coverage:
            self._coverage_frames += 1
            if self._coverage_frames >= self._ready_frames:
                self._lifecycle = "ready"
                self._diagnostic = None
        elif self._lifecycle not in {"refreshing", "degraded"}:
            self._coverage_frames = 0
            self._lifecycle = (
                "streaming"
                if self._resident_tiles > 0 or self._loading_tiles > 0
                else "connecting"
            )
        return TileRenderObservation(
            snapshot=self.snapshot(), action=TileLifecycleAction()
        )

    def mark_refresh_command_failed(self) -> None:
        self._clear_replacement()
        self._recoverable_degradation = False
        self._lifecycle = "degraded"
        self._diagnostic = "streamed-world replacement creation failed"

    def _has_textured_coverage(self) -> bool:
        return has_textured_coverage(
            visible_tiles=self._visible_tiles,
            geometries_rendered=self._geometries_rendered,
            materials_loaded=self._materials_loaded,
        )

    def _clear_replacement(self) -> None:
        self._replacement_tileset_path = None
        self._replacement_loaded_generation = None
        self._replacement_baseline = None
        self._replacement_frames = 0
        self._coverage_frames = 0

    def snapshot(self) -> TileLifecycleSnapshot:
        return TileLifecycleSnapshot(
            lifecycle=self._lifecycle,
            provider_generation=self._provider_generation,
            event_sequence=self._event_sequence,
            refresh_count=self._refresh_count,
            resident_tiles=self._resident_tiles,
            visible_tiles=self._visible_tiles,
            loading_tiles=self._loading_tiles,
            geometries_loaded=self._geometries_loaded,
            geometries_rendered=self._geometries_rendered,
            materials_loaded=self._materials_loaded,
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
