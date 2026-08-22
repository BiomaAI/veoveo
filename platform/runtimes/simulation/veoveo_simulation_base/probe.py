"""Certify the canonical simulation tuple and an overlay on NVIDIA hardware."""

from __future__ import annotations

import argparse
import hashlib
import importlib
import importlib.metadata
import json
import math
import subprocess
import sys
import time
import traceback
from pathlib import Path
from typing import Any

import numpy as np

from .contract import BUILD_LOCK_DIGEST_PATH, read_build_lock

RESULT_MARKER = "VEOVEO_SIMULATION_PROBE_RESULT="
STEP_MARKER = "VEOVEO_SIMULATION_PROBE_STEP="
OVERLAY_IDENTITY_PATH = Path("/opt/veoveo/simulation-overlay/identity.json")
ISAAC_LAB_REVISION_PATH = Path("/opt/veoveo/isaaclab/.veoveo-source-revision")
RTX_NVRTC_ROOT = (
    Path("/isaac-sim")
    / "extsDeprecated/omni.isaac.ml_archive/pip_prebundle"
    / "nvidia/cuda_nvrtc/lib"
)
RTX_NVRTC_BUILTINS = (
    "libnvrtc-builtins.so.12.8",
    "libnvrtc-builtins.alt.so.12.8",
)
SYNTHETIC_TORCH_MODULES = {
    ("torch.classes", "_classes.py"),
    ("torch.ops", "_ops.py"),
}


def _duration(started: float) -> int:
    return max(1, round((time.perf_counter() - started) * 1000))


def _is_below(path: str, root: Path) -> bool:
    try:
        Path(path).resolve().relative_to(root)
    except ValueError:
        return False
    return True


def _component_map(lock: dict[str, Any]) -> dict[str, dict[str, Any]]:
    return {item["component"]: item for item in lock["components"]}


def _driver_version() -> str:
    output = subprocess.run(
        [
            "nvidia-smi",
            "--query-gpu=driver_version",
            "--format=csv,noheader",
        ],
        check=True,
        capture_output=True,
        text=True,
    ).stdout
    versions = {line.strip() for line in output.splitlines() if line.strip()}
    if len(versions) != 1:
        raise RuntimeError(
            f"expected one NVIDIA driver version, received {sorted(versions)}"
        )
    return versions.pop()


def _verify_tuple(
    lock: dict[str, Any],
) -> tuple[dict[str, str], object, object, object]:
    import mujoco
    import mujoco_warp

    print(STEP_MARKER + "component_tuple.mujoco", flush=True)
    import newton

    print(STEP_MARKER + "component_tuple.newton", flush=True)
    import omni.kit.app
    import torch
    import warp as wp

    print(STEP_MARKER + "component_tuple.warp", flush=True)

    import isaaclab  # noqa: F401 - importability is part of the contract.
    import isaaclab_newton  # noqa: F401 - Newton integration must import.

    print(STEP_MARKER + "component_tuple.isaac_lab", flush=True)

    expected = _component_map(lock)
    observed: dict[str, str] = {}
    observed["isaac_sim"] = (
        Path("/isaac-sim/VERSION").read_text(encoding="utf-8").strip()
    )
    print(STEP_MARKER + "component_tuple.version.isaac_sim", flush=True)
    observed["isaac_lab"] = expected["isaac_lab"]["version"]
    print(STEP_MARKER + "component_tuple.version.isaac_lab", flush=True)
    observed["warp"] = wp.__version__
    print(STEP_MARKER + "component_tuple.version.warp", flush=True)
    observed["newton"] = newton.__version__
    print(STEP_MARKER + "component_tuple.version.newton", flush=True)
    observed["mujoco"] = importlib.metadata.version("mujoco")
    print(STEP_MARKER + "component_tuple.version.mujoco", flush=True)
    observed["mujoco_warp"] = importlib.metadata.version("mujoco-warp")
    print(STEP_MARKER + "component_tuple.version.mujoco_warp", flush=True)
    observed["python"] = ".".join(str(part) for part in sys.version_info[:3])
    observed["torch"] = torch.__version__
    observed["cuda"] = str(torch.version.cuda)
    missing_rtx_nvrtc = [
        library
        for library in RTX_NVRTC_BUILTINS
        if not (RTX_NVRTC_ROOT / library).is_file()
    ]
    if missing_rtx_nvrtc:
        raise RuntimeError(f"Isaac RTX NVRTC builtins are missing: {missing_rtx_nvrtc}")
    observed["isaac_rtx_nvrtc"] = expected["isaac_rtx_nvrtc"]["version"]
    observed["kit"] = expected["kit"]["version"]
    for name, value in observed.items():
        if value != expected[name]["version"]:
            raise RuntimeError(
                f"runtime component {name} differs: expected "
                f"{expected[name]['version']}, received {value}"
            )
    print(STEP_MARKER + "component_tuple.versions", flush=True)
    revision = ISAAC_LAB_REVISION_PATH.read_text(encoding="utf-8").strip()
    if revision != expected["isaac_lab"]["revision"]:
        raise RuntimeError(
            f"Isaac Lab source revision differs: expected "
            f"{expected['isaac_lab']['revision']}, received {revision}"
        )
    print(STEP_MARKER + "component_tuple.revision", flush=True)
    kit_version = omni.kit.app.get_app().get_kit_version()
    if not kit_version.startswith(expected["kit"]["version"]):
        raise RuntimeError(
            f"Kit version differs: expected {expected['kit']['version']}, "
            f"received {kit_version}"
        )
    print(STEP_MARKER + "component_tuple.kit", flush=True)
    if not wp.get_device("cuda:0").is_cuda:
        raise RuntimeError("Warp did not expose cuda:0")
    print(STEP_MARKER + "component_tuple.cuda", flush=True)
    return observed, wp, newton, torch


def _verify_module_graph(wp: object, newton: object, torch: object) -> dict[str, str]:
    roots = {
        "torch": Path(torch.__file__).resolve().parent,
        "warp": Path(wp.__file__).resolve().parent,
        "newton": Path(newton.__file__).resolve().parent,
    }
    outside: list[tuple[str, str]] = []
    loaded = 0
    for name, module in sorted(sys.modules.items()):
        package = name.split(".", maxsplit=1)[0]
        if package not in roots:
            continue
        module_path = getattr(module, "__file__", None)
        if not module_path:
            continue
        if (name, module_path) in SYNTHETIC_TORCH_MODULES:
            continue
        loaded += 1
        if not _is_below(module_path, roots[package]):
            outside.append((name, module_path))
    if loaded == 0 or outside:
        raise RuntimeError(f"mixed Torch/Warp/Newton module graph detected: {outside}")
    return {name: str(root) for name, root in roots.items()}


def _verify_simulation_manager_newton(app: object, wp: object) -> dict[str, Any]:
    import isaacsim.physics.newton
    import omni.timeline
    import omni.usd
    from isaacsim.core.experimental.prims import RigidPrim
    from isaacsim.core.simulation_manager import SimulationManager
    from pxr import Gf, UsdGeom, UsdPhysics

    stage = omni.usd.get_context().get_stage()
    UsdGeom.Xform.Define(stage, "/World")
    scene = UsdPhysics.Scene.Define(stage, "/World/PhysicsScene")
    scene.CreateGravityDirectionAttr(Gf.Vec3f(0.0, 0.0, -1.0))
    scene.CreateGravityMagnitudeAttr(9.80665)
    body_path = "/World/VeoveoNewtonManagerProbe"
    cube = UsdGeom.Cube.Define(stage, body_path)
    cube.CreateSizeAttr(0.5)
    cube.AddTranslateOp().Set(Gf.Vec3d(0.0, 0.0, 2.0))
    prim = cube.GetPrim()
    UsdPhysics.RigidBodyAPI.Apply(prim)
    UsdPhysics.CollisionAPI.Apply(prim)
    mass = UsdPhysics.MassAPI.Apply(prim)
    mass.CreateMassAttr(1.0)

    if not SimulationManager.switch_physics_engine("newton"):
        raise RuntimeError("SimulationManager could not select Newton")
    SimulationManager.setup_simulation(dt=1.0 / 120.0, device="cuda:0")
    newton_stage = isaacsim.physics.newton.acquire_stage()
    if newton_stage is None:
        raise RuntimeError("Isaac Sim did not expose the Newton stage")
    newton_stage.cfg.time_step_app = False
    timeline = omni.timeline.get_timeline_interface()
    timeline.play()
    app.update()
    SimulationManager.initialize_physics()
    if SimulationManager.get_active_physics_engine() != "newton":
        raise RuntimeError(
            "SimulationManager did not retain Newton after initialization"
        )
    simulation_view = SimulationManager.get_physics_simulation_view()
    if simulation_view is None or not simulation_view.is_valid():
        raise RuntimeError(
            "SimulationManager did not create a valid Newton tensor view"
        )

    rigid = RigidPrim([body_path], resolve_paths=True)
    if len(rigid) != 1 or not rigid.is_physics_tensor_entity_valid():
        raise RuntimeError(
            "Experimental RigidPrim did not resolve through Newton tensors"
        )
    positions, _ = rigid.get_world_poses()
    if not wp.get_device(positions.device).is_cuda:
        raise RuntimeError("Experimental RigidPrim did not retain CUDA tensor storage")
    initial_height = float(positions.numpy()[0, 2])
    forces = wp.array([[0.0, 0.0, 25.0]], dtype=wp.float32, device=positions.device)
    torques = wp.zeros((1, 3), dtype=wp.float32, device=positions.device)
    for _ in range(12):
        rigid.apply_forces_and_torques_at_pos(
            forces,
            torques,
            local_frame=False,
        )
        SimulationManager.step(steps=1, update_fabric=False)
    wp.synchronize_device(positions.device)
    final_positions, _ = rigid.get_world_poses()
    final_linear_velocities, _ = rigid.get_velocities()
    final_height = float(final_positions.numpy()[0, 2])
    final_vertical_velocity = float(final_linear_velocities.numpy()[0, 2])
    if final_height <= initial_height + 1.0e-3 or final_vertical_velocity <= 0.0:
        raise RuntimeError(
            "Experimental RigidPrim did not respond to a Newton CUDA force: "
            f"initial z={initial_height}, final z={final_height}, "
            f"final vz={final_vertical_velocity}"
        )
    return {
        "device": str(positions.device),
        "initialHeight": initial_height,
        "finalHeight": final_height,
        "finalVerticalVelocity": final_vertical_velocity,
        "forceNewtons": 25.0,
        "steps": 12,
    }


def _run_newton_camera(wp: object, newton: object, cameras: int) -> dict[str, Any]:
    from newton.sensors import SensorTiledCamera

    builder = newton.ModelBuilder(up_axis=newton.Axis.Z)
    body = builder.add_body(
        xform=wp.transform(wp.vec3(0.0, 0.0, 0.0), wp.quat_identity()),
        label="veoveo-simulation-probe-sphere",
    )
    builder.add_shape_sphere(body=body, radius=1.0, color=(1.0, 0.0, 0.0))
    model = builder.finalize(device="cuda:0")
    state = model.state()
    sensor = SensorTiledCamera(model, load_textures=False)
    width = 32
    height = 32
    fovs = np.full(cameras, math.radians(60.0), dtype=np.float32)
    rays = sensor.utils.compute_camera_rays_pinhole(
        width,
        height,
        camera_fovs=fovs,
    )
    image = sensor.utils.create_color_image_output(
        width,
        height,
        camera_count=cameras,
    )
    transforms = np.zeros((cameras, 1, 7), dtype=np.float32)
    transforms[:, 0, 0] = np.linspace(-1.0, 1.0, cameras)
    transforms[:, 0, 2] = 5.0
    transforms[:, 0, 6] = 1.0
    camera_transforms = wp.array(transforms, dtype=wp.transformf, device="cuda:0")
    sensor.update(state, camera_transforms, rays, color_image=image)
    wp.synchronize_device("cuda:0")
    pixels = image.numpy()
    frame_hashes = {
        hashlib.sha256(pixels[0, index].tobytes()).hexdigest()
        for index in range(cameras)
    }
    if (
        pixels.shape[:2] != (1, cameras)
        or np.unique(pixels).size < 2
        or len(frame_hashes) != cameras
    ):
        raise RuntimeError(
            "Newton tiled cameras produced no independent CUDA image variation"
        )
    return {
        "shape": list(pixels.shape),
        "uniquePixelValues": int(np.unique(pixels).size),
        "uniqueFrameHashes": len(frame_hashes),
    }


def _run_rtx_cameras(app: object, cameras: int) -> dict[str, Any]:
    import omni.replicator.core as rep

    width = 64
    height = 64
    annotators = []
    render_products = []
    try:
        with rep.new_layer():
            rep.create.plane(scale=20.0)
            rep.create.cube(position=(0.0, 0.0, 1.0), scale=1.5)
            rep.create.sphere(position=(2.0, 0.5, 0.8), scale=0.8)
            rep.create.cone(position=(-1.5, 1.5, 1.0), scale=1.0)
            rep.create.light(
                light_type="Dome",
                intensity=1200.0,
                color=(0.8, 0.9, 1.0),
            )
            rep.create.light(
                light_type="Sphere",
                position=(3.0, -4.0, 7.0),
                intensity=50000.0,
                color=(1.0, 0.75, 0.55),
            )
            for index in range(cameras):
                angle = 2.0 * math.pi * index / cameras
                radius = 7.0 + 0.5 * (index % 3)
                camera = rep.create.camera(
                    position=(
                        radius * math.cos(angle),
                        radius * math.sin(angle),
                        2.5 + 0.15 * (index % 5),
                    ),
                    look_at=(0.0, 0.3, 0.9),
                    focal_length=35.0,
                )
                render_product = rep.create.render_product(
                    camera,
                    resolution=(width, height),
                )
                annotator = rep.AnnotatorRegistry.get_annotator("rgb")
                annotator.attach(render_product)
                render_products.append(render_product)
                annotators.append(annotator)
        for _ in range(4):
            rep.orchestrator.step(rt_subframes=2)
        images = []
        for annotator in annotators:
            image = np.asarray(annotator.get_data())
            if image.shape == (height, width, 4):
                image = image[:, :, :3]
            if image.shape != (height, width, 3) or not np.issubdtype(
                image.dtype, np.number
            ):
                raise RuntimeError(
                    f"RTX camera returned invalid RGB data {image.shape}"
                )
            images.append(image.copy())
        batch = np.stack(images, axis=0)
        standard_deviations = batch.astype(np.float32).std(axis=(1, 2, 3))
        hashes = {hashlib.sha256(image.tobytes()).hexdigest() for image in images}
        if float(standard_deviations.min()) < 1.0 or len(hashes) != cameras:
            raise RuntimeError("RTX cameras did not produce independent visual content")
        app.update()
        return {
            "shape": list(batch.shape),
            "minimumStandardDeviation": float(standard_deviations.min()),
            "uniqueFrameHashes": len(hashes),
        }
    finally:
        for annotator, render_product in zip(annotators, render_products, strict=True):
            annotator.detach(render_product)


def _verify_overlay(expected_kind: str, wp: object) -> dict[str, str]:
    identity = json.loads(OVERLAY_IDENTITY_PATH.read_text(encoding="utf-8"))
    lock_digest = BUILD_LOCK_DIGEST_PATH.read_text(encoding="utf-8").strip()
    if identity.get("schemaVersion") != "veoveo.io/simulation-overlay-identity/v1":
        raise RuntimeError("overlay identity has an unsupported schema")
    if identity.get("kind") != expected_kind:
        raise RuntimeError(
            f"overlay kind differs: expected {expected_kind}, received {identity.get('kind')}"
        )
    if identity.get("baseLockDigest") != f"sha256:{lock_digest}":
        raise RuntimeError("overlay was not built against the embedded base lock")
    module_name = identity.get("probeModule")
    if not isinstance(module_name, str) or not module_name:
        raise RuntimeError("overlay identity omitted probeModule")
    module = importlib.import_module(module_name)
    marker = module.prove_overlay(wp)
    if marker != identity.get("marker"):
        raise RuntimeError("overlay probe returned the wrong identity marker")
    return {
        "kind": expected_kind,
        "module": module_name,
        "marker": marker,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--overlay-kind",
        required=True,
        choices=("first_party_uav", "anonymous_external"),
    )
    parser.add_argument("--cameras", type=int, default=20)
    args = parser.parse_args()
    if args.cameras < 20:
        parser.error("hardware certification requires at least 20 independent cameras")

    from isaacsim import SimulationApp

    app = SimulationApp(
        {
            "headless": True,
            "hide_ui": True,
            "renderer": "RaytracedLighting",
            "width": 64,
            "height": 64,
            "extra_args": [
                "--enable",
                "isaacsim.physics.newton",
                "--/exts/isaacsim.core.simulation_manager/default_engine=newton",
            ],
        }
    )
    try:
        lock = read_build_lock()
        print(STEP_MARKER + "component_tuple", flush=True)
        started = time.perf_counter()
        components, wp, newton, torch = _verify_tuple(lock)
        tuple_duration = _duration(started)

        print(STEP_MARKER + "module_graph", flush=True)
        started = time.perf_counter()
        module_roots = _verify_module_graph(wp, newton, torch)
        module_duration = _duration(started)

        print(STEP_MARKER + "newton_dynamics", flush=True)
        started = time.perf_counter()
        newton_dynamics = _verify_simulation_manager_newton(app, wp)
        newton_dynamics_duration = _duration(started)

        print(STEP_MARKER + "newton_tiled_camera", flush=True)
        started = time.perf_counter()
        newton_camera = _run_newton_camera(wp, newton, args.cameras)
        newton_duration = _duration(started)

        print(STEP_MARKER + "independent_rtx_cameras", flush=True)
        started = time.perf_counter()
        rtx = _run_rtx_cameras(app, args.cameras)
        rtx_duration = _duration(started)

        print(STEP_MARKER + "overlay_boundary", flush=True)
        started = time.perf_counter()
        overlay = _verify_overlay(args.overlay_kind, wp)
        overlay_duration = _duration(started)

        device = wp.get_device("cuda:0")
        result = {
            "components": components,
            "hardware": {
                "gpuName": f"NVIDIA {device.name}"
                if "nvidia" not in device.name.lower()
                else device.name,
                "driverVersion": _driver_version(),
                "cudaDevice": "cuda:0",
                "graphicsApi": "Vulkan",
                "renderer": "RaytracedLighting",
            },
            "cameraCount": args.cameras,
            "moduleRoots": module_roots,
            "newtonCamera": newton_camera,
            "newtonDynamics": newton_dynamics,
            "rtx": rtx,
            "overlay": overlay,
            "probeDurationsMilliseconds": {
                "componentTuple": tuple_duration,
                "moduleGraph": module_duration,
                "newtonDynamics": newton_dynamics_duration,
                "newtonTiledCamera": newton_duration,
                "independentRtxCameras": rtx_duration,
                "overlayBoundary": overlay_duration,
            },
        }
        print(RESULT_MARKER + json.dumps(result, sort_keys=True), flush=True)
    except BaseException:
        # Kit owns the process shutdown path and can otherwise hide Python's
        # uncaught-exception report while returning a successful exit status.
        traceback.print_exc()
        raise
    finally:
        app.close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
