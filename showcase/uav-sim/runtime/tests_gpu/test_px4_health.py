"""CUDA HIL with native PX4; no cluster, rendering, or arming is required.

Requires the repository-qualified PX4 tree in UAV_SIM_PX4_DIRECTORY, CUDA, MAVLINK20=1,
and free local ports for PX4 instance 41. Run with a 120-second outer timeout.
The process, writable root and bridge belong to this fixture and close on failure.
"""
from __future__ import annotations

import json
import os
from pathlib import Path
import re
import subprocess
import threading
import time
import unittest

from test_plant import CudaPlant, NativeSensorProfile
from veoveo_uav_sim.px4_hil import Px4HilBridge
from veoveo_uav_sim.vehicle_spec import decode_hil_packet


class Px4SensorHealthTests(unittest.TestCase):
    def test_stationary_cuda_measurements_keep_all_px4_validators_healthy(self) -> None:
        self.assertEqual(os.environ.get("MAVLINK20"), "1", "MAVLink 2 is required")
        root = Path(os.environ["UAV_SIM_PX4_DIRECTORY"])
        profile = NativeSensorProfile.selected()
        plant = CudaPlant(fleet_size=1, physics_hz=profile.physics_hz)
        plant.sample(1)  # Finish CUDA compilation before PX4 starts its sensor deadline.
        bridge = Px4HilBridge(str(root), instance=41)
        stop = threading.Event()
        failures: list[BaseException] = []

        def publish() -> None:
            try:
                started = time.monotonic()
                packet = None
                for step in range(1, profile.hil_hz * 45 + 1):
                    if stop.is_set():
                        return
                    plant_step = profile.plant_step(step)
                    if plant_step is not None:
                        packet = plant.sample(plant_step)[0]
                    frame, _ = decode_hil_packet(
                        packet, time_usec=round(step * 1_000_000 / profile.hil_hz),
                        fields_updated=profile.cadence.fields_updated(profile.hil_hz, step),
                        gps_updated=profile.cadence.gps_due(profile.hil_hz, step),
                    )
                    bridge.publish(step, frame)
                    bridge.raise_if_failed()
                    stop.wait(max(0.0, started + step / profile.hil_hz - time.monotonic()))
            except BaseException as error:
                failures.append(error)
                stop.set()

        producer = threading.Thread(target=publish, name="px4-test-cuda", daemon=True)
        try:
            bridge.start()
            producer.start()
            # Two observations exceed PX4's repeated-value window even at 30 Hz.
            for _ in range(2):
                self.assertFalse(stop.wait(15.0), f"HIL publisher failed: {failures}")
                bridge.raise_if_failed()
                self.assertTrue(bridge.connected, "PX4 did not connect to the HIL bridge")
                status = subprocess.run(
                    [str(root / "build/px4_sitl_default/bin/px4-sensors"),
                     "--instance", "41", "status"],
                    capture_output=True, text=True, timeout=5.0, check=True,
                ).stdout
                print(status, flush=True)
                self.assertEqual(re.findall(r"sensor #\d+, prio: \d+, state: (\w+)", status),
                                 ["OK"] * 5, status)
                self.assertNotIn("failsafe: YES", status)
                self.assertNotRegex(status, r"(?:accel|gyro) data gap: [1-9]\d* events")
                for selected in ("selected gyro:", "selected accel:", "selected MAG:", "selected BARO:"):
                    self.assertIn(selected, status)
            self.assertFalse(failures, str(failures))
        finally:
            stop.set()
            if producer.ident is not None:
                producer.join(timeout=5.0)
            bridge.close()


if __name__ == "__main__":
    result = unittest.TextTestRunner(verbosity=2).run(
        unittest.defaultTestLoader.loadTestsFromTestCase(Px4SensorHealthTests)
    )
    print(json.dumps({
        "suite": "uav-native-px4-sensor-health", "passed": result.wasSuccessful(),
        "tests": result.testsRun, "failures": len(result.failures), "errors": len(result.errors),
    }))
    raise SystemExit(0 if result.wasSuccessful() else 1)
