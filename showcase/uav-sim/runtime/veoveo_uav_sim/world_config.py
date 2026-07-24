from __future__ import annotations

import copy
import threading
from dataclasses import dataclass
from typing import Any


class WorldConfigurationError(ValueError):
    pass


def _object(value: Any, context: str) -> dict[str, Any]:
    if not isinstance(value, dict) or not all(isinstance(key, str) for key in value):
        raise WorldConfigurationError(f"{context} must be a JSON object")
    return value


def _exact_fields(
    value: dict[str, Any], required: set[str], context: str
) -> None:
    actual = set(value)
    if actual != required:
        missing = sorted(required - actual)
        unknown = sorted(actual - required)
        raise WorldConfigurationError(
            f"{context} fields invalid; missing={missing}, unknown={unknown}"
        )


def _string(value: Any, field: str, maximum: int = 1_024) -> str:
    if not isinstance(value, str) or not 1 <= len(value) <= maximum:
        raise WorldConfigurationError(
            f"{field} must be a non-empty string of at most {maximum} characters"
        )
    return value


def _number(value: Any, field: str, minimum: float, maximum: float) -> float:
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise WorldConfigurationError(f"{field} must be a number")
    result = float(value)
    if not minimum <= result <= maximum:
        raise WorldConfigurationError(
            f"{field} must be between {minimum} and {maximum}"
        )
    return result


@dataclass(frozen=True, slots=True)
class GeoreferenceOrigin:
    latitude_degrees: float
    longitude_degrees: float
    ellipsoid_height_m: float

    def as_dict(self) -> dict[str, float]:
        return {
            "latitude_degrees": self.latitude_degrees,
            "longitude_degrees": self.longitude_degrees,
            "ellipsoid_height_m": self.ellipsoid_height_m,
        }


@dataclass(frozen=True, slots=True)
class WorldConfiguration:
    revision_uri: str
    spec_sha256: str
    simulation_frame_uri: str
    georeference_origin: GeoreferenceOrigin

    @classmethod
    def from_request(
        cls, payload: Any, expected_session_id: str
    ) -> "WorldConfiguration":
        request = _object(payload, "world configuration")
        _exact_fields(request, {"session_id", "world"}, "world configuration")
        if request["session_id"] != expected_session_id:
            raise WorldConfigurationError(
                f"unknown simulation session {request['session_id']!r}"
            )
        world = _object(request["world"], "world")
        _exact_fields(
            world,
            {
                "revision_uri",
                "spec_sha256",
                "simulation_frame_uri",
                "georeference_origin",
            },
            "world",
        )
        revision_uri = _string(world["revision_uri"], "revision_uri")
        if (
            not revision_uri.startswith("frames://world/")
            or "/revision/" not in revision_uri
            or revision_uri.endswith("/revision/")
        ):
            raise WorldConfigurationError(
                "revision_uri must use "
                "frames://world/{world_id}/revision/{revision_id}"
            )
        simulation_frame_uri = _string(
            world["simulation_frame_uri"], "simulation_frame_uri"
        )
        if not simulation_frame_uri.startswith(f"{revision_uri}/frame/"):
            raise WorldConfigurationError(
                "simulation_frame_uri must identify a frame in revision_uri"
            )
        spec_sha256 = _string(world["spec_sha256"], "spec_sha256", 64)
        if len(spec_sha256) != 64 or any(
            character not in "0123456789abcdef" for character in spec_sha256
        ):
            raise WorldConfigurationError(
                "spec_sha256 must be a lowercase SHA-256 digest"
            )
        origin = _object(world["georeference_origin"], "georeference_origin")
        _exact_fields(
            origin,
            {
                "latitude_degrees",
                "longitude_degrees",
                "ellipsoid_height_m",
            },
            "georeference_origin",
        )
        return cls(
            revision_uri=revision_uri,
            spec_sha256=spec_sha256,
            simulation_frame_uri=simulation_frame_uri,
            georeference_origin=GeoreferenceOrigin(
                latitude_degrees=_number(
                    origin["latitude_degrees"],
                    "latitude_degrees",
                    -90.0,
                    90.0,
                ),
                longitude_degrees=_number(
                    origin["longitude_degrees"],
                    "longitude_degrees",
                    -180.0,
                    180.0,
                ),
                ellipsoid_height_m=_number(
                    origin["ellipsoid_height_m"],
                    "ellipsoid_height_m",
                    -1_000.0,
                    100_000.0,
                ),
            ),
        )

    def as_dict(self) -> dict[str, object]:
        return {
            "revision_uri": self.revision_uri,
            "spec_sha256": self.spec_sha256,
            "simulation_frame_uri": self.simulation_frame_uri,
            "georeference_origin": self.georeference_origin.as_dict(),
        }


class WorldConfigurationSlot:
    def __init__(self) -> None:
        self._condition = threading.Condition()
        self._world: WorldConfiguration | None = None

    def configure(self, world: WorldConfiguration) -> WorldConfiguration:
        with self._condition:
            if self._world is not None and self._world != world:
                raise WorldConfigurationError(
                    "the simulation is already bound to a different world revision"
                )
            if self._world is None:
                self._world = world
                self._condition.notify_all()
            return self._world

    def get(self) -> WorldConfiguration | None:
        with self._condition:
            return copy.deepcopy(self._world)

    def wait(self, timeout_seconds: float) -> WorldConfiguration | None:
        with self._condition:
            self._condition.wait_for(
                lambda: self._world is not None, timeout=timeout_seconds
            )
            return copy.deepcopy(self._world)
