"""CUDA plant qualification; invoke separately from the runtime's CPU unit suite."""
from __future__ import annotations

import json
import os
from dataclasses import dataclass
import unittest

import numpy as np
import warp as wp

from veoveo_uav_sim.plant_warp import PACKET_WIDTH, advance_fleet_and_sample_hil
from veoveo_uav_sim.magnetic_warp import upload_magnetic_tables
from veoveo_uav_sim.vehicle_spec import (
    PX4_IRIS_IMU_NOISE_REFERENCE_HZ, PX4_IRIS_SENSOR_CADENCE, SensorCadence, decode_hil_packet,
)


@dataclass(frozen=True)
class NativeSensorProfile:
    name: str
    physics_hz: int
    hil_hz: int
    cadence: SensorCadence

    @classmethod
    def selected(cls) -> "NativeSensorProfile":
        name = os.environ.get("UAV_SIM_PX4_SENSOR_PROFILE", "held-30-60")
        if name == "held-30-60":
            return cls(name, 30, 60, PX4_IRIS_SENSOR_CADENCE)
        if name == "new-sample-250":
            cadence = SensorCadence(imu_hz=250, barometer_hz=50, magnetometer_hz=50, gps_hz=10)
            cadence.validate_for_physics(250)
            return cls(name, 250, 250, cadence)
        raise ValueError("UAV_SIM_PX4_SENSOR_PROFILE must be held-30-60 or new-sample-250")

    def plant_step(self, hil_step: int) -> int | None:
        # Preserve the original odd-step sample + held even-step baseline.
        ratio = self.hil_hz // self.physics_hz
        return (hil_step - 1) // ratio + 1 if (hil_step - 1) % ratio == 0 else None


class CudaPlant:
    """Isolated CUDA fixture shared by distribution and native PX4 qualification."""

    def __init__(
        self, fleet_size: int = 4, rotor_speed: float = 0.0,
        latitude: float = 40.758, longitude: float = -73.9855,
        physics_hz: int = 30,
    ) -> None:
        wp.init()
        self.device = wp.get_device("cuda:0")
        if not self.device.is_cuda:
            raise RuntimeError("plant qualification requires a hardware CUDA device")
        self.fleet_size = fleet_size
        self.latitude = latitude
        self.longitude = longitude
        self.physics_hz = physics_hz
        self.magnetic_model = upload_magnetic_tables(self.device)
        self.controls = wp.full((fleet_size, 4), rotor_speed, dtype=wp.float32, device=self.device)
        self.motors = wp.zeros_like(self.controls)
        self.indices = wp.array(list(range(fleet_size)), dtype=wp.int32, device=self.device)
        self.poses = wp.array(
            [wp.transform(wp.vec3(0.0, 0.0, 0.04), wp.quat_identity())] * fleet_size,
            dtype=wp.transform, device=self.device,
        )
        self.velocities = wp.zeros(fleet_size, dtype=wp.spatial_vector, device=self.device)
        self.previous = wp.zeros((fleet_size, 3), dtype=wp.float32, device=self.device)
        self.packet = wp.zeros((fleet_size, PACKET_WIDTH), dtype=wp.float32, device=self.device)

    def sample(self, step: int) -> np.ndarray:
        wp.launch(
            advance_fleet_and_sample_hil, dim=self.fleet_size,
            inputs=[self.controls, self.motors, self.indices, self.poses, self.velocities,
                    self.previous, self.packet, self.magnetic_model, step, 1.0 / self.physics_hz,
                    self.latitude, self.longitude, -17.0, 111_000.0, 84_000.0],
            device=self.device,
        )
        # Observation and the PX4 wire adapter require this readback; physics stays on CUDA.
        return self.packet.numpy().copy()


class GpuSensorTests(unittest.TestCase):
    def samples(self, rotor_speed: float = 0.0, seed_offset: int = 0) -> np.ndarray:
        plant = CudaPlant(rotor_speed=rotor_speed)
        return np.stack([plant.sample(step + seed_offset) for step in range(1, 601)])

    def assert_noise(self, noise: np.ndarray, expected_std: np.ndarray) -> None:
        normalized = noise / expected_std
        self.assertTrue(np.all(np.abs(normalized.mean(axis=0)) < 0.15))
        self.assertTrue(np.all(normalized.std(axis=0) > 0.85))
        self.assertTrue(np.all(normalized.std(axis=0) < 1.15))
        # Detect accidental reuse of the same Gaussian draw across axes or vehicles.
        correlations = np.corrcoef(normalized.reshape(len(noise), -1).T)
        np.fill_diagonal(correlations, 0.0)
        self.assertLess(float(np.abs(correlations).max()), 0.2)

    def test_stationary_sensor_noise_is_seeded_and_preserves_ground_truth(self) -> None:
        samples = self.samples()
        np.testing.assert_array_equal(samples, self.samples())
        self.assertTrue(np.isfinite(samples).all())
        truth = samples[:, :, list(range(16)) + list(range(20, 30))]
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

        noise = np.concatenate((
            # Pinned PX4 WMM at this fixture's GPS origin, transformed ENU -> FRD.
            samples[:, :, 16:19] - np.array([-0.044694892, -0.205321782, 0.463847402]),
            samples[:, :, 30:33] - samples[:, :, 10:13],
            samples[:, :, 33:36] - samples[:, :, 13:16],
        ), axis=2)
        scale = np.sqrt(30 / PX4_IRIS_IMU_NOISE_REFERENCE_HZ)
        self.assert_noise(noise, np.concatenate((
            np.array([0.02, 0.02, 0.03]),
            np.array([0.01, 0.01, 0.01, 0.1, 0.1, 0.1]) * scale,
        )))
        for packet in samples[:, 0]:
            frame, snapshot = decode_hil_packet(
                packet, time_usec=1, fields_updated=8191, gps_updated=True,
            )
            self.assertEqual(snapshot.angular_velocity_frd_rps, (0.0, 0.0, 0.0))
            self.assertNotEqual(frame.angular_velocity_frd_rps, snapshot.angular_velocity_frd_rps)
            self.assertNotEqual(frame.acceleration_frd_mps2, snapshot.linear_acceleration_frd_mps2)

    def test_powered_noise_matches_px4_and_cannot_change_dynamics(self) -> None:
        samples = self.samples(rotor_speed=100.0)
        other_noise = self.samples(rotor_speed=100.0, seed_offset=1000)
        truth_columns = list(range(16)) + list(range(20, 30))
        np.testing.assert_array_equal(samples[:, :, truth_columns], other_noise[:, :, truth_columns])
        self.assertFalse(np.array_equal(samples[:, :, 30:36], other_noise[:, :, 30:36]))
        noise = samples[:, :, 30:36] - samples[:, :, 10:16]
        scale = np.sqrt(30 / PX4_IRIS_IMU_NOISE_REFERENCE_HZ)
        self.assert_noise(noise, np.array([0.14, 0.07, 0.03, 0.5, 1.7, 1.4]) * scale)

    def test_imu_integrated_noise_variance_is_independent_of_physics_rate(self) -> None:
        expected = np.square([0.14, 0.07, 0.03, 0.5, 1.7, 1.4]) / PX4_IRIS_IMU_NOISE_REFERENCE_HZ
        for physics_hz in (30, 60, 250):
            with self.subTest(physics_hz=physics_hz):
                plant = CudaPlant(rotor_speed=100.0, physics_hz=physics_hz)
                samples = np.stack([plant.sample(step) for step in range(1, 601)])
                noise = samples[:, :, 30:36] - samples[:, :, 10:16]
                # Variance accumulated per second is sample variance times dt.
                measured = noise.var(axis=(0, 1)) / physics_hz
                np.testing.assert_allclose(measured, expected, rtol=0.1)


if __name__ == "__main__":
    result = unittest.TextTestRunner(verbosity=2).run(
        unittest.defaultTestLoader.loadTestsFromTestCase(GpuSensorTests)
    )
    print(json.dumps({
        "suite": "uav-gpu-plant", "passed": result.wasSuccessful(),
        "tests": result.testsRun, "failures": len(result.failures),
        "errors": len(result.errors), "warp": wp.__version__,
    }))
    raise SystemExit(0 if result.wasSuccessful() else 1)
