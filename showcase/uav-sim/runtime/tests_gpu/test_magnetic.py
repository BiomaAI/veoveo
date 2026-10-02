"""CUDA magnetic model and body-frame qualification against pinned PX4 values."""
from __future__ import annotations

import json
import math
import unittest

import numpy as np
import warp as wp

from test_plant import CudaPlant
from veoveo_uav_sim.magnetic_warp import magnetic_field_enu_gauss, upload_magnetic_tables


@wp.kernel
def sample_fields(
    model: wp.array3d(dtype=wp.float32), points: wp.array2d(dtype=wp.float32),
    fields: wp.array(dtype=wp.vec3),
):
    index = wp.tid()
    fields[index] = magnetic_field_enu_gauss(model, points[index, 0], points[index, 1])


class MagneticFieldTests(unittest.TestCase):
    def test_cuda_field_agrees_with_pinned_px4_across_globe_and_date_line(self) -> None:
        # Computed independently with the pinned geo_mag_declination.cpp and its
        # original int16 tables; values are ENU Gauss, not a Python model oracle.
        cases = (
            (40.758, -73.9855, (-0.044694892, 0.205321782, -0.463847402)),
            (0.0, 0.0, (-0.018983380, 0.275143364, 0.161185965)),
            (-33.87, 151.21, (0.055901766, 0.241731962, 0.512113793)),
            (47.4, 8.5, (0.012893577, 0.217435223, -0.430388012)),
            (64.15, -21.94, (-0.026950248, 0.130678058, -0.507754142)),
            (-34.6, -58.4, (-0.028747590, 0.171125537, 0.149393595)),
            (89.0, 180.0, (-0.009703217, -0.017320271, -0.569323797)),
            (-89.0, -180.0, (0.090510506, -0.137568037, 0.524866578)),
            (30.0, 179.9, (0.029681686, 0.269118458, -0.255458944)),
            (30.0, -180.1, (0.029681686, 0.269118458, -0.255458944)),
            (30.0, -179.9, (0.029895121, 0.268867627, -0.255619312)),
            (30.0, 180.1, (0.029895121, 0.268867627, -0.255619312)),
        )
        wp.init()
        device = wp.get_device("cuda:0")
        model = upload_magnetic_tables(device)
        points = wp.array([case[:2] for case in cases], dtype=wp.float32, device=device)
        fields = wp.empty(len(cases), dtype=wp.vec3, device=device)
        wp.launch(sample_fields, dim=len(cases), inputs=[model, points, fields], device=device)
        np.testing.assert_allclose(fields.numpy(), [case[2] for case in cases], atol=2e-7, rtol=2e-6)

    def test_hil_field_follows_location_and_body_heading(self) -> None:
        for latitude, longitude, enu in (
            (40.758, -73.9855, (-0.044694892, 0.205321782, -0.463847402)),
            (-33.87, 151.21, (0.055901766, 0.241731962, 0.512113793)),
        ):
            with self.subTest(latitude=latitude, longitude=longitude):
                plant = CudaPlant(fleet_size=2, latitude=latitude, longitude=longitude)
                # Vehicle 0 points east; vehicle 1 points north. Both report FRD.
                north_heading = wp.quat(0.0, 0.0, math.sqrt(0.5), math.sqrt(0.5))
                plant.poses.assign(wp.array([
                    wp.transform(wp.vec3(0.0, 0.0, 0.04), wp.quat_identity()),
                    wp.transform(wp.vec3(0.0, 0.0, 0.04), north_heading),
                ], dtype=wp.transform, device=plant.device))
                samples = np.stack([plant.sample(step) for step in range(1, 601)])
                expected = np.array([(enu[0], -enu[1], -enu[2]), (enu[1], enu[0], -enu[2])])
                np.testing.assert_allclose(samples[:, :, 16:19].mean(axis=0), expected, atol=0.004)
                self.assertTrue(np.all(samples[:, :, 16:19].std(axis=0) > 0.017))

    def test_model_rejects_cpu_execution(self) -> None:
        with self.assertRaisesRegex(ValueError, "hardware CUDA"):
            upload_magnetic_tables(wp.get_device("cpu"))


if __name__ == "__main__":
    result = unittest.TextTestRunner(verbosity=2).run(
        unittest.defaultTestLoader.loadTestsFromTestCase(MagneticFieldTests)
    )
    print(json.dumps({
        "suite": "uav-gpu-magnetic-model", "passed": result.wasSuccessful(),
        "tests": result.testsRun, "failures": len(result.failures), "errors": len(result.errors),
    }))
    raise SystemExit(0 if result.wasSuccessful() else 1)
