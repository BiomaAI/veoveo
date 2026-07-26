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
        raise RuntimeError(f"expected one NVIDIA driver version, received {sorted(versions)}")
    return versions.pop()


def _verify_tuple(lock: dict[str, Any]) -> tuple[dict[str, str], object, object]:
    import mujoco
    import mujoco_warp
    print(STEP_MARKER + "component_tuple.mujoco", flush=True)
    import newton
    print(STEP_MARKER + "component_tuple.newton", flush=True)
    import omni.kit.app
    import warp as wp
    print(STEP_MARKER + "component_tuple.warp", flush=True)

    import isaaclab  # noqa: F401 - importability is part of the contract.
    import isaaclab_newton  # noqa: F401 - Newton integration must import.
    print(STEP_MARKER + "component_tuple.isaac_lab", flush=True)

    expected = _component_map(lock)
    observed: dict[str, str] = {}
    observed["isaac_sim"] = Path("/isaac-sim/VERSION").read_text(
        encoding="utf-8"
    ).strip()
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
    observed["cuda"] = "12.9"
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
    return observed, wp, newton


def _verify_module_graph(wp: object, newton: object) -> dict[str, str]:
    roots = {
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
        loaded += 1
        if not _is_below(module_path, roots[package]):
            outside.append((name, module_path))
    if loaded == 0 or outside:
        raise RuntimeError(f"mixed Warp/Newton module graph detected: {outside}")
    return {name: str(root) for name, root in roots.items()}


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
        raise RuntimeError("Newton tiled cameras produced no independent CUDA image variation")
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
                raise RuntimeError(f"RTX camera returned invalid RGB data {image.shape}")
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
        }
    )
    try:
        lock = read_build_lock()
        print(STEP_MARKER + "component_tuple", flush=True)
        started = time.perf_counter()
        components, wp, newton = _verify_tuple(lock)
        tuple_duration = _duration(started)

        print(STEP_MARKER + "module_graph", flush=True)
        started = time.perf_counter()
        module_roots = _verify_module_graph(wp, newton)
        module_duration = _duration(started)

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
            "rtx": rtx,
            "overlay": overlay,
            "probeDurationsMilliseconds": {
                "componentTuple": tuple_duration,
                "moduleGraph": module_duration,
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
