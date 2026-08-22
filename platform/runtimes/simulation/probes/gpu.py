#!/usr/bin/env python3
"""Run hardware-only conformance for the canonical simulation runtime."""

from __future__ import annotations

import argparse
import ctypes
import json
import math
import os
import re
import subprocess
import tempfile
import time
from pathlib import Path

import numpy as np

from identity import inspect_identity

MINIMUM_DRIVER = (570, 169)
QUALIFIED_DRIVER = (595, 58, 3)


def _phase(name: str) -> None:
    print(f"SIMULATION_RUNTIME_PHASE={name}", flush=True)


def _verify_runtime_storage() -> dict[str, object]:
    writable_paths = [
        Path("/isaac-sim/kit/cache"),
        Path("/isaac-sim/kit/data"),
        Path(os.environ["XDG_CACHE_HOME"]),
        Path(os.environ["XDG_DATA_HOME"]),
    ]
    for directory in writable_paths:
        directory.mkdir(parents=True, exist_ok=True)
        with tempfile.NamedTemporaryFile(dir=directory) as probe:
            probe.write(b"veoveo-simulation-runtime")
            probe.flush()

    shared_memory = os.statvfs("/dev/shm")
    shared_memory_bytes = shared_memory.f_frsize * shared_memory.f_blocks
    if shared_memory_bytes < 2 * 1024 * 1024 * 1024:
        raise RuntimeError(
            "simulation runtime conformance requires a private /dev/shm of at "
            f"least 2 GiB, received {shared_memory_bytes} bytes"
        )
    return {
        "writable_paths": [str(path) for path in writable_paths],
        "shared_memory_bytes": shared_memory_bytes,
    }


def _verify_driver_apis() -> dict[str, int]:
    try:
        cuda = ctypes.CDLL("libcuda.so.1")
        nvenc = ctypes.CDLL("libnvidia-encode.so.1")
    except OSError as error:
        raise RuntimeError("NVIDIA CUDA and NVENC driver libraries are required") from error

    cuda.cuInit.argtypes = [ctypes.c_uint]
    cuda.cuInit.restype = ctypes.c_int
    cuda.cuDeviceGetCount.argtypes = [ctypes.POINTER(ctypes.c_int)]
    cuda.cuDeviceGetCount.restype = ctypes.c_int
    device_count = ctypes.c_int()
    if cuda.cuInit(0) != 0:
        raise RuntimeError("NVIDIA CUDA driver initialization failed")
    if cuda.cuDeviceGetCount(ctypes.byref(device_count)) != 0:
        raise RuntimeError("NVIDIA CUDA device enumeration failed")
    if device_count.value < 1:
        raise RuntimeError("an accessible NVIDIA CUDA device is required")

    nvenc.NvEncodeAPIGetMaxSupportedVersion.argtypes = [
        ctypes.POINTER(ctypes.c_uint32)
    ]
    nvenc.NvEncodeAPIGetMaxSupportedVersion.restype = ctypes.c_int
    nvenc_version = ctypes.c_uint32()
    if nvenc.NvEncodeAPIGetMaxSupportedVersion(ctypes.byref(nvenc_version)) != 0:
        raise RuntimeError("the NVIDIA NVENC API did not report a supported version")
    if not hasattr(nvenc, "NvEncodeAPICreateInstance"):
        raise RuntimeError("the NVIDIA NVENC session API is unavailable")
    return {
        "cuda_device_count": device_count.value,
        "nvenc_api_version": nvenc_version.value,
    }


def _verify_hardware_identity() -> dict[str, object]:
    completed = subprocess.run(
        [
            "nvidia-smi",
            "--query-gpu=uuid,name,driver_version",
            "--format=csv,noheader,nounits",
        ],
        check=True,
        capture_output=True,
        text=True,
    )
    records = [
        [field.strip() for field in line.split(",")]
        for line in completed.stdout.splitlines()
        if line.strip()
    ]
    if len(records) != 1 or len(records[0]) != 3:
        raise RuntimeError(
            "simulation runtime conformance requires exactly one visible NVIDIA GPU"
        )
    gpu_uuid, gpu_name, driver_version = records[0]
    version = tuple(int(part) for part in driver_version.split("."))
    if version < MINIMUM_DRIVER:
        raise RuntimeError(
            f"NVIDIA driver {driver_version} is below the 570.169 runtime floor"
        )
    return {
        "gpu_uuid": gpu_uuid,
        "gpu_name": gpu_name,
        "driver_version": driver_version,
        "minimum_driver_version": "570.169",
        "qualified_driver_version": "595.58.03",
        "release_driver_qualified": version >= QUALIFIED_DRIVER,
    }


def _verify_cuda_kernels() -> dict[str, object]:
    import newton
    import torch
    import warp as wp
    from newton.sensors import SensorTiledCamera

    if not torch.cuda.is_available():
        raise RuntimeError("Torch cannot access a hardware CUDA device")
    device = torch.device("cuda:0")
    torch_input = torch.arange(1, 4097, dtype=torch.float32, device=device)
    torch_output = torch_input.square().sum()
    if not torch.isfinite(torch_output).item():
        raise RuntimeError("Torch CUDA kernel produced a non-finite result")

    @wp.kernel
    def affine_kernel(values: wp.array(dtype=wp.float32)):
        index = wp.tid()
        values[index] = wp.float32(index) * 2.0 + 1.0

    warp_output = wp.zeros(4096, dtype=wp.float32, device="cuda:0")
    wp.launch(affine_kernel, dim=warp_output.shape, inputs=[warp_output], device="cuda:0")
    wp.synchronize_device("cuda:0")
    warp_tensor = wp.to_torch(warp_output)
    if warp_tensor.device.type != "cuda":
        raise RuntimeError(f"Warp result is not CUDA-resident: {warp_tensor.device}")
    expected_sum = float(4096 * 4096)
    actual_sum = float(warp_tensor.sum().item())
    if actual_sum != expected_sum:
        raise RuntimeError(
            f"Warp CUDA kernel returned {actual_sum}, expected {expected_sum}"
        )

    builder = newton.ModelBuilder(up_axis=newton.Axis.Z)
    body = builder.add_body(
        xform=wp.transform(wp.vec3(0.0, 0.0, 3.0), wp.quat_identity()),
        label="simulation-runtime-probe",
    )
    builder.add_shape_sphere(body=body, radius=1.0, color=(1.0, 0.1, 0.1))
    model = builder.finalize(device="cuda:0")
    state = model.state()
    next_state = model.state()
    control = model.control()
    solver = newton.solvers.SolverMuJoCo(model, disable_contacts=True)
    initial_height = float(state.body_q.numpy()[body, 2])
    for _ in range(8):
        state.clear_forces()
        solver.step(state, next_state, control, None, 1.0 / 120.0)
        state, next_state = next_state, state
    wp.synchronize_device("cuda:0")
    final_height = float(state.body_q.numpy()[body, 2])
    if not final_height < initial_height - 1.0e-4:
        raise RuntimeError(
            "Newton SolverMuJoCo did not advance the CUDA-resident rigid body: "
            f"initial z={initial_height}, final z={final_height}"
        )

    sensor = SensorTiledCamera(model, load_textures=False)
    camera_count = 4
    width = 32
    height = 24
    rays = sensor.utils.compute_camera_rays_pinhole(
        width,
        height,
        camera_fovs=np.full(
            camera_count,
            math.radians(60.0),
            dtype=np.float32,
        ),
    )
    image = sensor.utils.create_color_image_output(
        width,
        height,
        camera_count=camera_count,
    )
    transforms = torch.zeros(
        (camera_count, 1, 7),
        dtype=torch.float32,
        device=device,
    )
    transforms[:, 0, 2] = 5.0
    transforms[:, 0, 6] = 1.0
    cameras = wp.from_torch(transforms, dtype=wp.transformf)
    sensor.update(state, cameras, rays, color_image=image)
    wp.synchronize_device("cuda:0")
    image_tensor = wp.to_torch(image)
    if image_tensor.device.type != "cuda":
        raise RuntimeError(
            f"Newton tiled-camera result is not CUDA-resident: {image_tensor.device}"
        )
    unique_values = int(torch.unique(image_tensor).numel())
    if unique_values < 2:
        raise RuntimeError("Newton tiled camera produced no visible variation")

    properties = torch.cuda.get_device_properties(device)
    return {
        "device": properties.name,
        "compute_capability": f"{properties.major}.{properties.minor}",
        "torch_cuda": torch.version.cuda,
        "torch_kernel_sum": float(torch_output.item()),
        "warp_kernel_sum": actual_sum,
        "newton_solver_mujoco": {
            "device": str(model.device),
            "initial_height": initial_height,
            "final_height": final_height,
            "steps": 8,
        },
        "newton_tiled_camera": {
            "shape": list(image_tensor.shape),
            "device": str(image_tensor.device),
            "unique_values": unique_values,
        },
    }


def _verify_isaac_lab_camera(
    camera_count: int,
    width: int,
    height: int,
    warmup_frames: int,
) -> dict[str, object]:
    import torch

    import isaaclab.sim as sim_utils
    from isaaclab.sensors.camera import Camera, CameraCfg

    sim_utils.create_new_stage()
    simulation = sim_utils.SimulationContext(
        sim_utils.SimulationCfg(dt=0.01, device="cuda:0")
    )

    ground = sim_utils.CuboidCfg(
        size=(20.0, 20.0, 0.1),
        visual_material=sim_utils.PreviewSurfaceCfg(
            diffuse_color=(0.15, 0.15, 0.15)
        ),
    )
    ground.func("/World/Ground", ground, translation=(0.0, 0.0, -0.05))
    light = sim_utils.DomeLightCfg(intensity=1200.0, color=(0.8, 0.9, 1.0))
    light.func("/World/DomeLight", light)

    for index in range(camera_count):
        angle = 2.0 * math.pi * index / camera_count
        sim_utils.create_prim(
            f"/World/CameraRig_{index}",
            "Xform",
            translation=(2.5 * math.cos(angle), 2.5 * math.sin(angle), 0.0),
        )
        cube = sim_utils.CuboidCfg(
            size=(0.5 + 0.1 * index, 0.5, 0.5),
            visual_material=sim_utils.PreviewSurfaceCfg(
                diffuse_color=(
                    0.2 + 0.15 * index,
                    0.8 - 0.1 * index,
                    0.25 + 0.1 * index,
                )
            ),
        )
        cube.func(
            f"/World/ProbeCube_{index}",
            cube,
            translation=(
                1.5 * math.cos(angle),
                1.5 * math.sin(angle),
                0.5,
            ),
        )

    camera_cfg = CameraCfg(
        height=height,
        width=width,
        offset=CameraCfg.OffsetCfg(
            pos=(0.0, 0.0, 5.0),
            rot=(0.0, 1.0, 0.0, 0.0),
            convention="ros",
        ),
        prim_path="/World/CameraRig_.*/Camera",
        update_period=0,
        data_types=["rgb"],
        spawn=sim_utils.PinholeCameraCfg(
            focal_length=24.0,
            focus_distance=400.0,
            horizontal_aperture=20.955,
            clipping_range=(0.1, 1000.0),
        ),
    )
    camera = Camera(camera_cfg)
    try:
        simulation.reset()
        started = time.perf_counter()
        for _ in range(warmup_frames):
            simulation.step()
            camera.update(simulation.cfg.dt)
        rgb = camera.data.output["rgb"]
        rgb_tensor = rgb.torch if hasattr(rgb, "torch") else rgb
        if rgb_tensor.device.type != "cuda":
            raise RuntimeError(
                f"Isaac Lab RGB output is not CUDA-resident: {rgb_tensor.device}"
            )
        expected_shape = (camera_count, height, width, 3)
        if tuple(rgb_tensor.shape) != expected_shape:
            raise RuntimeError(
                f"expected Isaac Lab RGB shape {expected_shape}, "
                f"received {tuple(rgb_tensor.shape)}"
            )
        pixels = rgb_tensor.to(dtype=torch.float32)
        standard_deviation = pixels.std(dim=(1, 2, 3))
        if torch.any(standard_deviation < 1.0).item():
            raise RuntimeError(
                "one or more Isaac Lab RTX cameras produced blank output"
            )
        means = pixels.mean(dim=(1, 2, 3))
        if torch.unique(torch.round(means * 1000.0) / 1000.0).numel() < 2:
            raise RuntimeError("Isaac Lab RTX camera views are not distinct")
        return {
            "shape": list(rgb_tensor.shape),
            "dtype": str(rgb_tensor.dtype),
            "device": str(rgb_tensor.device),
            "minimum_standard_deviation": float(standard_deviation.min().item()),
            "maximum_standard_deviation": float(standard_deviation.max().item()),
            "distinct_mean_values": int(
                torch.unique(torch.round(means * 1000.0) / 1000.0).numel()
            ),
            "elapsed_seconds": time.perf_counter() - started,
        }
    finally:
        del camera
        simulation.stop()
        simulation.clear_instance()


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--cameras", type=int, default=4)
    parser.add_argument("--width", type=int, default=160)
    parser.add_argument("--height", type=int, default=120)
    parser.add_argument("--warmup-frames", type=int, default=20)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--image-digest", required=True)
    args = parser.parse_args()
    if min(args.cameras, args.width, args.height, args.warmup_frames) < 1:
        parser.error("camera count, dimensions, and warmup frames must be positive")
    if re.fullmatch(r"sha256:[0-9a-f]{64}", args.image_digest) is None:
        parser.error("--image-digest must be a lowercase sha256 digest")

    _phase("runtime_storage")
    storage = _verify_runtime_storage()
    _phase("driver_apis")
    driver_apis = _verify_driver_apis()
    _phase("hardware_identity")
    hardware = _verify_hardware_identity()

    from isaaclab.app import AppLauncher

    _phase("app_launcher")
    simulation_app = AppLauncher(headless=True, enable_cameras=True).app
    try:
        _phase("module_identity")
        identity = inspect_identity()
        _phase("cuda_kernels")
        cuda = _verify_cuda_kernels()
        _phase("isaac_lab_camera")
        camera = _verify_isaac_lab_camera(
            args.cameras,
            args.width,
            args.height,
            args.warmup_frames,
        )
        result = {
            "schema_version": "veoveo.io/simulation-runtime-conformance/v1",
            "profile": os.environ["VEOVEO_SIMULATION_RUNTIME_PROFILE"],
            "image_digest": args.image_digest,
            "identity": identity,
            "storage": storage,
            "driver_apis": driver_apis,
            "hardware": hardware,
            "cuda": cuda,
            "isaac_lab_camera": camera,
        }
        encoded = json.dumps(result, indent=2, sort_keys=True) + "\n"
        if args.output is not None:
            args.output.write_text(encoded)
        print(
            "SIMULATION_RUNTIME_GPU_CONFORMANCE="
            + json.dumps(result, sort_keys=True),
            flush=True,
        )
    finally:
        simulation_app.close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
