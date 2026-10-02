"""CUDA plant and native PX4 takeoff, movement, landing and re-arm qualification.

Requires the pinned patched PX4 tree in UAV_SIM_PX4_DIRECTORY, hardware CUDA,
MAVLINK20=1, and free ports for instance 42. Run with a 600-second outer timeout.
The fixture owns its process, sockets, thread and temporary writable root.
"""
from __future__ import annotations

import json
import os
from pathlib import Path
import subprocess
import shutil
import threading
import time
import unittest

import numpy as np

from test_plant import CudaPlant
from veoveo_uav_sim.contracts import Waypoint
from veoveo_uav_sim.px4 import CommandDeadline, Px4Commander
from veoveo_uav_sim.px4_hil import Px4HilBridge
from veoveo_uav_sim.vehicle_spec import (
    PX4_HIL_HZ, PX4_IRIS_IMU_NOISE_REFERENCE_HZ, PX4_IRIS_SENSOR_CADENCE, decode_hil_packet,
)


class Px4FlightTests(unittest.TestCase):
    def test_cuda_flight_can_land_and_rearm_with_healthy_compass(self) -> None:
        self.assertEqual(os.environ.get("MAVLINK20"), "1", "MAVLink 2 is required")
        root = Path(os.environ["UAV_SIM_PX4_DIRECTORY"])
        instance = 42
        plant = CudaPlant(fleet_size=1)
        plant.sample(1)  # CUDA compilation must finish before the sensor deadline starts.
        bridge = Px4HilBridge(str(root), instance=instance)
        commander = Px4Commander(instance=instance, origin_height_m=-17.0)
        stop = threading.Event()
        failures: list[BaseException] = []
        observations: list[dict[str, object]] = []
        latest: list[np.ndarray] = []
        samples: list[np.ndarray] = []

        def publish() -> None:
            try:
                started = time.monotonic()
                packet = None
                for step in range(1, PX4_HIL_HZ * 540 + 1):
                    if stop.is_set():
                        return
                    if step % 2 == 1:
                        plant.controls.assign(np.asarray([bridge.controls()], dtype=np.float32))
                        packet = plant.sample((step + 1) // 2)[0]
                        latest[:] = [packet]
                        samples.append(np.concatenate(([step / PX4_HIL_HZ], packet)))
                    frame, _ = decode_hil_packet(
                        packet, time_usec=round(step * 1_000_000 / PX4_HIL_HZ),
                        fields_updated=PX4_IRIS_SENSOR_CADENCE.fields_updated(PX4_HIL_HZ, step),
                        gps_updated=PX4_IRIS_SENSOR_CADENCE.gps_due(PX4_HIL_HZ, step),
                    )
                    bridge.publish(step, frame)
                    bridge.raise_if_failed()
                    stop.wait(max(0.0, started + step / PX4_HIL_HZ - time.monotonic()))
                raise TimeoutError("CUDA flight publisher exceeded 540 seconds")
            except BaseException as error:
                failures.append(error)
                stop.set()

        def topic(name: str) -> str:
            return subprocess.run(
                [str(root / "build/px4_sitl_default/bin/px4-listener"),
                 "--instance", str(instance), name, "-n", "1"],
                capture_output=True, text=True, timeout=5.0, check=True,
            ).stdout

        def wait_for(label: str, seconds: float, predicate) -> None:
            deadline = time.monotonic() + seconds
            while time.monotonic() < deadline:
                self.assertFalse(failures, str(failures))
                bridge.raise_if_failed()
                commander.status()  # Keep the production GCS heartbeat active.
                if predicate():
                    observation = {"phase": label, "position_enu_m": latest[0][:3].tolist()}
                    observations.append(observation)
                    print(json.dumps(observation), flush=True)
                    return
                self.assertFalse(stop.wait(0.25), str(failures))
            self.fail(f"{label} timed out; {topic('vehicle_status')}; {topic('estimator_status_flags')}")

        producer = threading.Thread(target=publish, name="px4-flight-cuda", daemon=True)
        try:
            bridge.start()
            producer.start()
            commander.connect(timeout_seconds=15.0)
            wait_for("initial preflight", 60.0, lambda: "pre_flight_checks_pass: True" in topic("vehicle_status"))
            for name, expected in (("IMU_INTEG_RATE", PX4_IRIS_IMU_NOISE_REFERENCE_HZ),
                                   ("IMU_GYRO_RATEMAX", 800)):
                value = subprocess.run(
                    [str(root / "build/px4_sitl_default/bin/px4-param"),
                     "--instance", str(instance), "show", name],
                    capture_output=True, text=True, timeout=5.0, check=True,
                ).stdout
                self.assertRegex(value, rf"{name}.*:\s*{expected}\s", value)
            for cycle in range(2):
                commander.takeoff(8.0, deadline=CommandDeadline.after(30.0))
                wait_for(f"takeoff {cycle + 1}", 45.0, lambda: bool(latest) and latest[0][2] > 6.0)
                # Changes in horizontal velocity make EKF yaw observable against GPS.
                route = tuple(
                    Waypoint(40.758 + north / 111_000.0, -73.9855 + east / 84_000.0,
                             -9.0, 5.0, 1.0)
                    for east, north in ((40.0, 0.0), (40.0, 40.0), (0.0, 0.0))
                )
                self.assertEqual(commander.execute_mission(route, timeout_seconds=100.0), len(route))
                self.assertIn("cs_mag_fault: False", topic("estimator_status_flags"))
                commander.land(deadline=CommandDeadline.after(20.0))
                wait_for(f"landed {cycle + 1}", 60.0,
                         lambda: commander.status().flight_state in {"landed", "standby"}
                         and bool(latest) and latest[0][2] < 0.1)
                wait_for(f"postflight preflight {cycle + 1}", 20.0,
                         lambda: "pre_flight_checks_pass: True" in topic("vehicle_status"))
                self.assertIn("cs_mag_fault: False", topic("estimator_status_flags"))
            self.assertFalse(failures, str(failures))
        finally:
            # Simulator process teardown ends any unresolved simulated operation;
            # this fixture never shares a vehicle with an installation or another test.
            stop.set()
            if producer.ident is not None:
                producer.join(timeout=5.0)
            try:
                capture = os.environ.get("UAV_SIM_PX4_FLIGHT_LOG_DIRECTORY")
                if capture:
                    # These isolated simulation logs contain no installation credentials.
                    destination = Path(capture)
                    destination.mkdir(parents=True, exist_ok=True)
                    np.save(destination / "hil-samples.npy", np.asarray(samples))
                    for name in ("vehicle_attitude", "vehicle_local_position",
                                 "estimator_status_flags", "estimator_sensor_bias",
                                 "yaw_estimator_status", "vehicle_status"):
                        try:
                            (destination / f"{name}.txt").write_text(topic(name))
                        except (OSError, subprocess.SubprocessError) as error:
                            print(f"diagnostic topic {name} unavailable: {error}", flush=True)
                    for log in bridge._process.command.working_directory.glob("log/**/*.ulg"):
                        shutil.copyfile(log, destination / log.name)
            finally:
                commander.close()
                bridge.close()
            self.assertFalse(producer.is_alive(), "CUDA flight publisher did not terminate")


if __name__ == "__main__":
    result = unittest.TextTestRunner(verbosity=2).run(
        unittest.defaultTestLoader.loadTestsFromTestCase(Px4FlightTests)
    )
    print(json.dumps({
        "suite": "uav-native-px4-flight", "passed": result.wasSuccessful(),
        "tests": result.testsRun, "failures": len(result.failures), "errors": len(result.errors),
    }))
    raise SystemExit(0 if result.wasSuccessful() else 1)
