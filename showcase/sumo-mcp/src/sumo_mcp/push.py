"""sumo-sim: step the simulation and push each frame into the Recording Hub.

The push spine, runnable standalone (fake driver by default, or a real SUMO
connection). Deterministic with the fake driver so the push smoke can assert
what landed in the hub.
"""

from __future__ import annotations

import argparse

from .sim_driver import FakeSimDriver, SimDriver
from .streams import RerunPublisher


def run_push(driver: SimDriver, publisher: RerunPublisher, steps: int) -> int:
    """Advance `steps` and publish every frame; returns frames published."""
    for step in range(steps):
        driver.step(1)
        publisher.publish(
            step,
            driver.vehicles(),
            driver.signals(),
            driver.mean_speed(),
        )
    publisher.flush()
    return steps


def main() -> None:
    parser = argparse.ArgumentParser(description="Push SUMO world state into the hub")
    parser.add_argument("--proxy", default="rerun+http://127.0.0.1:9876/proxy")
    parser.add_argument("--application-id", default="veoveo-sumo")
    parser.add_argument("--recording", default="sumo-run")
    parser.add_argument("--steps", type=int, default=60)
    parser.add_argument("--vehicles", type=int, default=12)
    parser.add_argument("--seed", type=int, default=1)
    args = parser.parse_args()

    driver: SimDriver = FakeSimDriver(n_vehicles=args.vehicles, seed=args.seed)
    publisher = RerunPublisher(args.proxy, args.application_id, args.recording)
    n = run_push(driver, publisher, args.steps)
    print(f"pushed {n} frames as recording {args.recording}")


if __name__ == "__main__":
    main()
