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

from .sim_driver import SignalState, VehicleState

WORLD_TIMELINE = "tick"


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
        signals: Sequence[SignalState],
        mean_speed: float,
    ) -> None:
        rr = self._rr
        rr.set_time(WORLD_TIMELINE, sequence=step, recording=self._stream)
        for v in vehicles:
            rr.log(
                f"/world/sumo/vehicle/{v.id}",
                rr.GeoPoints(lat_lon=[[v.lat, v.lon]]),
                recording=self._stream,
            )
            rr.log(
                f"/world/sumo/vehicle/{v.id}/speed",
                rr.Scalars([v.speed_mps]),
                recording=self._stream,
            )
        for s in signals:
            rr.log(
                f"/world/sumo/signal/{s.id}",
                rr.Scalars([float(s.phase)]),
                recording=self._stream,
            )
        rr.log(
            "/world/sumo/mean_speed",
            rr.Scalars([mean_speed]),
            recording=self._stream,
        )

    def flush(self) -> None:
        self._stream.flush()
