"""The push path: SUMO world state → typed Rerun streams → the Recording Hub.

The sim owner publishes each step as world-model components under
`/world/sumo/**` — vehicles as GeoPoints (real lat/lon), speeds and signal
phases as scalars — into the hub's gRPC proxy. This is the "tiny simulator
pushing to rerun" spine: SUMO is one more producer; the hub never pulls.

The rerun SDK is an optional dependency, imported lazily, so the MCP server and
its tests work without it.
"""

from __future__ import annotations

from collections.abc import Sequence

from .sim_driver import VehicleState

WORLD_TIMELINE = "tick"

# Free-flow reference for colouring: at/above this a car is "green", at 0 "red".
_FREE_FLOW_MPS = 14.0


def _speed_color(speed_mps: float) -> list[int]:
    """Congested (red) → free-flowing (green), so a jam reads at a glance."""
    t = max(0.0, min(1.0, speed_mps / _FREE_FLOW_MPS))
    return [int(235 * (1.0 - t)) + 20, int(200 * t) + 40, 70]


class RerunPublisher:
    """Publishes SUMO state into the hub as one Rerun recording (one session)."""

    def __init__(
        self,
        proxy_url: str,
        application_id: str = "veoveo-sumo",
        recording: str = "sumo-run",
    ) -> None:
        import rerun as rr  # lazy: only needed on the push path

        self._rr = rr
        self._stream = rr.RecordingStream(application_id, recording_id=recording)
        rr.connect_grpc(proxy_url, recording=self._stream)

    def publish(
        self,
        step: int,
        vehicles: Sequence[VehicleState],
        mean_speed: float,
        vehicle_count: int,
    ) -> None:
        rr = self._rr
        rr.set_time(WORLD_TIMELINE, sequence=step, recording=self._stream)
        # Every vehicle as one geo point cloud, coloured by speed — one log call
        # per frame, so a dense city stays smooth and reads as a live map.
        rr.log(
            "/world/sumo/vehicles",
            rr.GeoPoints(
                lat_lon=[[v.lat, v.lon] for v in vehicles],
                colors=[_speed_color(v.speed_mps) for v in vehicles],
                radii=[-4.0],  # 4 UI points, so cars stay visible at any zoom
            ),
            recording=self._stream,
        )
        rr.log("/world/sumo/mean_speed", rr.Scalars([mean_speed]), recording=self._stream)
        rr.log(
            "/world/sumo/vehicle_count",
            rr.Scalars([float(vehicle_count)]),
            recording=self._stream,
        )

    def flush(self) -> None:
        self._stream.flush()
