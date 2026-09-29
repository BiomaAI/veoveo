"""CUDA plant qualification; invoke separately from the runtime's CPU unit suite."""
from __future__ import annotations

import json
import unittest

import numpy as np
import warp as wp

from veoveo_uav_sim.plant_warp import PACKET_WIDTH, advance_fleet_and_sample_hil


class GpuBarometerTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        wp.init()
        cls.device = wp.get_device("cuda:0")
        if not cls.device.is_cuda:
            raise RuntimeError("plant qualification requires a hardware CUDA device")

    def samples(self) -> np.ndarray:
        fleet_size = 4
        controls = wp.zeros((fleet_size, 4), dtype=wp.float32, device=self.device)
        motors = wp.zeros_like(controls)
        indices = wp.array(list(range(fleet_size)), dtype=wp.int32, device=self.device)
        poses = wp.array(
            [wp.transform(wp.vec3(0.0, 0.0, 0.04), wp.quat_identity())] * fleet_size,
            dtype=wp.transform, device=self.device,
        )
        velocities = wp.zeros(fleet_size, dtype=wp.spatial_vector, device=self.device)
        previous = wp.zeros((fleet_size, 3), dtype=wp.float32, device=self.device)
        packet = wp.zeros((fleet_size, PACKET_WIDTH), dtype=wp.float32, device=self.device)
        observed = []
        for step in range(1, 601):
            wp.launch(
                advance_fleet_and_sample_hil, dim=fleet_size,
                inputs=[controls, motors, indices, poses, velocities, previous, packet,
                        step, 1.0 / 30.0, 40.758, -73.9855, -17.0, 111_000.0, 84_000.0],
                device=self.device,
            )
            # Readback is test observation only; the production plant stays on CUDA.
            observed.append(packet.numpy().copy())
        return np.stack(observed)

    def test_stationary_sensor_noise_is_seeded_and_preserves_ground_truth(self) -> None:
        samples = self.samples()
        np.testing.assert_array_equal(samples, self.samples())
        self.assertTrue(np.isfinite(samples).all())
        truth = np.delete(samples, 19, axis=2)
        np.testing.assert_array_equal(truth, np.broadcast_to(truth[0], truth.shape))
        pressure = samples[:, :, 19]
        self.assertTrue(np.all(pressure.std(axis=0) > 0.008))
        self.assertTrue(np.all(pressure.std(axis=0) < 0.012))
        self.assertFalse(np.array_equal(pressure[:, 0], pressure[:, 1]))
        altitude = float(samples[0, 0, 24])
        ideal_pressure = 1013.25 / (288.15 / (288.15 - 0.0065 * altitude)) ** 5.2561
        self.assertTrue(np.all(np.abs(pressure.mean(axis=0) - ideal_pressure) < 0.003))
        for vehicle in range(pressure.shape[1]):
            equal_count = 0
            maximum_equal = 0
            for previous, current in zip(pressure[:-1, vehicle], pressure[1:, vehicle]):
                equal_count = equal_count + 1 if previous == current else 0
                maximum_equal = max(maximum_equal, equal_count)
            self.assertLess(maximum_equal, 10)


if __name__ == "__main__":
    result = unittest.TextTestRunner(verbosity=2).run(
        unittest.defaultTestLoader.loadTestsFromTestCase(GpuBarometerTests)
    )
    print(json.dumps({
        "suite": "uav-gpu-plant", "passed": result.wasSuccessful(),
        "tests": result.testsRun, "failures": len(result.failures),
        "errors": len(result.errors), "warp": wp.__version__,
    }))
    raise SystemExit(0 if result.wasSuccessful() else 1)
