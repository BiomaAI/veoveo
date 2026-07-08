"""The event plane: watch conditions over the sim exposed as MCP resources.

Congestion, arrival, and collision are conditions the agent should be *woken*
by, not poll for. They surface as subscribable resources; when a condition
crosses its threshold the server sends `resources/updated`, which the kernel
turns into a wake. This module holds the (pure, testable) evaluation; the server
wires the resource list/read/subscribe and the push.
"""

from __future__ import annotations

from pydantic import BaseModel, ConfigDict

from .sim_driver import SimDriver

CONGESTION_URI = "sim://congestion"

# Mean speed (m/s) below which the network is considered congested.
CONGESTION_THRESHOLD_MPS = 3.0


class CongestionState(BaseModel):
    model_config = ConfigDict(extra="forbid")

    uri: str = CONGESTION_URI
    sim_time_s: float
    mean_speed_mps: float
    congested: bool


def evaluate_congestion(driver: SimDriver) -> CongestionState:
    mean = driver.mean_speed()
    return CongestionState(
        sim_time_s=driver.sim_time(),
        mean_speed_mps=mean,
        congested=mean < CONGESTION_THRESHOLD_MPS,
    )
