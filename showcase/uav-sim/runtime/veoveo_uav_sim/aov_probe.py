from __future__ import annotations

import argparse
import logging
import traceback
from typing import Any

from .aov import (
    FOLLOW_CAMERA_RENDER_PRODUCT_NAME,
    FOLLOW_CAMERA_RENDER_PRODUCT_PATH,
    livestream_aov_arguments,
)
from .hydra_camera import HydraRgbCameraSensor, RtxHydraRenderProduct


LOGGER = logging.getLogger("veoveo.uav_sim.aov_probe")


def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Exercise the NVIDIA AOV render-product path in isolation."
    )
    parser.add_argument("--frames", type=int, default=600)
    parser.add_argument("--warmup-frames", type=int, default=120)
    parser.add_argument("--width", type=int, default=1280)
    parser.add_argument("--height", type=int, default=720)
    parser.add_argument("--fps", type=int, default=20)
    parser.add_argument("--signal-port", type=int, default=49100)
    parser.add_argument("--media-port", type=int, default=47998)
    return parser.parse_args()


def _create_scene(stage: Any) -> tuple[str, str]:
    from pxr import Gf, UsdGeom, UsdLux

    camera_path = "/World/AovProbeCamera"
    camera = UsdGeom.Camera.Define(stage, camera_path)
    camera.CreateFocalLengthAttr(35.0)
    camera.CreateHorizontalApertureAttr(36.0)
    camera.CreateClippingRangeAttr(Gf.Vec2f(0.1, 1_000.0))
    camera_transform = Gf.Matrix4d().SetLookAt(
        Gf.Vec3d(6.0, -6.0, 4.0),
        Gf.Vec3d(0.0, 0.0, 0.5),
        Gf.Vec3d(0.0, 0.0, 1.0),
    )
    UsdGeom.Xformable(camera.GetPrim()).AddTransformOp(
        precision=UsdGeom.XformOp.PrecisionDouble
    ).Set(camera_transform.GetInverse())

    sensor_camera_path = "/World/AovProbeSensor"
    sensor_camera = UsdGeom.Camera.Define(stage, sensor_camera_path)
    sensor_camera.CreateFocalLengthAttr(24.0)
    sensor_camera.CreateHorizontalApertureAttr(36.0)
    sensor_camera.CreateClippingRangeAttr(Gf.Vec2f(0.01, 1_000.0))
    sensor_camera_transform = Gf.Matrix4d().SetLookAt(
        Gf.Vec3d(0.0, -4.0, 2.0),
        Gf.Vec3d(0.0, 0.0, 0.0),
        Gf.Vec3d(0.0, 0.0, 1.0),
    )
    UsdGeom.Xformable(sensor_camera.GetPrim()).AddTransformOp(
        precision=UsdGeom.XformOp.PrecisionDouble
    ).Set(sensor_camera_transform.GetInverse())

    cube = UsdGeom.Cube.Define(stage, "/World/AovProbeCube")
    cube.CreateSizeAttr(2.0)
    cube.CreateDisplayColorAttr(
        [Gf.Vec3f(0.05, 0.7, 0.35)]
    )

    ground = UsdGeom.Cube.Define(stage, "/World/AovProbeGround")
    ground.CreateSizeAttr(1.0)
    ground_transform = UsdGeom.Xformable(ground.GetPrim())
    ground_transform.AddScaleOp().Set(Gf.Vec3f(14.0, 14.0, 0.1))
    ground_transform.AddTranslateOp().Set(Gf.Vec3d(0.0, 0.0, -1.05))
    ground.CreateDisplayColorAttr(
        [Gf.Vec3f(0.12, 0.14, 0.18)]
    )

    key_light = UsdLux.DistantLight.Define(stage, "/World/AovProbeKeyLight")
    key_light.CreateIntensityAttr(3_000.0)
    key_light.CreateAngleAttr(1.0)
    UsdGeom.Xformable(key_light.GetPrim()).AddRotateXYZOp().Set(
        Gf.Vec3f(35.0, -25.0, -30.0)
    )

    fill_light = UsdLux.DomeLight.Define(stage, "/World/AovProbeFillLight")
    fill_light.CreateIntensityAttr(500.0)
    return camera_path, sensor_camera_path


def run() -> None:
    args = _parse_args()
    if args.frames < 1 or args.warmup_frames < 1:
        raise ValueError("probe frame counts must be positive")

    from isaacsim import SimulationApp

    simulation_app = SimulationApp(
        {
            "headless": True,
            "renderer": "RaytracedLighting",
            "width": args.width,
            "height": args.height,
            "sync_loads": True,
            "extra_args": [
                "--enable",
                "omni.kit.livestream.webrtc",
                *livestream_aov_arguments(
                    signal_port=args.signal_port,
                    media_port=args.media_port,
                    public_ip="127.0.0.1",
                    target_fps=args.fps,
                ),
                "--portable-root",
                "/var/lib/veoveo/runtime-cache/aov-probe/kit-portable",
            ],
        }
    )

    follow_render_product: RtxHydraRenderProduct | None = None
    sensor: HydraRgbCameraSensor | None = None
    extension_manager: Any | None = None
    timeline: Any | None = None
    try:
        import carb.settings
        import omni.kit.app
        import omni.timeline
        import omni.usd

        settings = carb.settings.get_settings()
        configured_stream_type = settings.get(
            (
                "/exts/omni.kit.livestream.aov/"
                "Render.OmniverseKit.HydraTextures."
                "uav_follow_camera.LdrColor/"
                "spectatorStream/0/streamType"
            )
        )
        if configured_stream_type != "webrtc":
            raise RuntimeError(
                "dedicated AOV stream setting is missing: "
                f"{configured_stream_type!r}"
            )
        for viewport_index in range(2):
            implicit_stream_type = settings.get(
                (
                    "/exts/omni.kit.livestream.aov/"
                    "Render.OmniverseKit.HydraTextures."
                    "omni_kit_widget_viewport_"
                    f"ViewportTexture_{viewport_index}.LdrColor/"
                    "spectatorStream/0/streamType"
                )
            )
            if implicit_stream_type:
                raise RuntimeError(
                    "NVIDIA AOV package still declares an implicit viewport "
                    f"stream at index {viewport_index}"
                )
        print("AOV_PROBE_CONFIG explicit_streams=1", flush=True)

        context = omni.usd.get_context()
        context.new_stage()
        simulation_app.update()
        stage = context.get_stage()
        print("AOV_PROBE_STAGE_READY", flush=True)
        camera_path, sensor_camera_path = _create_scene(stage)
        print(f"AOV_PROBE_SCENE_READY camera={camera_path}", flush=True)
        sensor = HydraRgbCameraSensor(
            name="uav_nadir_camera",
            camera_path=sensor_camera_path,
            width=640,
            height=480,
            fps=args.fps,
        )
        print(
            (
                "AOV_PROBE_AUX_SENSOR_READY "
                f"render_product={sensor.render_product_path}"
            ),
            flush=True,
        )
        follow_render_product = RtxHydraRenderProduct(
            name=FOLLOW_CAMERA_RENDER_PRODUCT_NAME,
            camera_path=camera_path,
            width=args.width,
            height=args.height,
            fps=args.fps,
        )
        follow_render_product_path = follow_render_product.path
        if follow_render_product_path != FOLLOW_CAMERA_RENDER_PRODUCT_PATH:
            raise RuntimeError(
                "HydraTexture created an unexpected render product: "
                f"{follow_render_product_path}"
            )
        print(
            f"AOV_PROBE_RENDER_PRODUCT path={follow_render_product_path}",
            flush=True,
        )

        timeline = omni.timeline.get_timeline_interface()
        timeline.play()
        for _ in range(args.warmup_frames):
            simulation_app.update()
        sensor_frame = sensor.latest_frame()
        if sensor_frame is None:
            raise RuntimeError(
                "auxiliary RTX HydraTexture produced no captured LdrColor frame"
            )
        if sensor_frame.pixels.shape != (480, 640, 3):
            raise RuntimeError(
                "auxiliary RTX HydraTexture capture has an unexpected shape: "
                f"{sensor_frame.pixels.shape!r}"
            )
        if (
            int(sensor_frame.pixels.max())
            - int(sensor_frame.pixels.min())
            < 8
        ):
            raise RuntimeError(
                "auxiliary RTX HydraTexture captured no visible scene detail"
            )
        print(
            (
                "AOV_PROBE_AUX_SENSOR_STREAMING "
                f"frames={sensor_frame.sequence} "
                f"shape={sensor_frame.pixels.shape}"
            ),
            flush=True,
        )

        extension_manager = omni.kit.app.get_app().get_extension_manager()
        extension_manager.set_extension_enabled_immediate(
            "omni.kit.livestream.aov", True
        )
        if not extension_manager.is_extension_enabled("omni.kit.livestream.aov"):
            raise RuntimeError("failed to enable omni.kit.livestream.aov")

        print(
            (
                "AOV_PROBE_READY "
                f"render_product={follow_render_product_path} "
                f"resolution={args.width}x{args.height} fps={args.fps}"
            ),
            flush=True,
        )
        LOGGER.info(
            "AOV_PROBE_READY render_product=%s resolution=%dx%d fps=%d",
            follow_render_product_path,
            args.width,
            args.height,
            args.fps,
        )
        for _ in range(args.frames):
            simulation_app.update()
        sensor_frame_after_aov = sensor.latest_frame(
            after_sequence=sensor_frame.sequence
        )
        if sensor_frame_after_aov is None:
            raise RuntimeError(
                "auxiliary RTX HydraTexture stopped while AOV was active"
            )
        print(
            (
                f"AOV_PROBE_PASS frames={args.frames} "
                f"sensor_frames={sensor_frame_after_aov.sequence}"
            ),
            flush=True,
        )
        LOGGER.info("AOV_PROBE_PASS frames=%d", args.frames)
    except BaseException as error:
        print(
            f"AOV_PROBE_ERROR type={type(error).__name__} message={error}",
            flush=True,
        )
        traceback.print_exc()
        raise
    finally:
        if (
            extension_manager is not None
            and extension_manager.is_extension_enabled("omni.kit.livestream.aov")
        ):
            extension_manager.set_extension_enabled_immediate(
                "omni.kit.livestream.aov", False
            )
        if sensor is not None:
            sensor.close()
        if follow_render_product is not None:
            follow_render_product.close()
        if timeline is not None and timeline.is_playing():
            timeline.stop()
        simulation_app.close()


def main() -> None:
    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s %(levelname)s %(name)s %(message)s",
    )
    run()


if __name__ == "__main__":
    main()
