from __future__ import annotations


FOLLOW_CAMERA_RENDER_PRODUCT_NAME = "uav_follow_camera"
FOLLOW_CAMERA_RENDER_PRODUCT_PATH = (
    f"/Render/OmniverseKit/HydraTextures/{FOLLOW_CAMERA_RENDER_PRODUCT_NAME}"
)
FOLLOW_CAMERA_LDR_COLOR_AOV = (
    "Render.OmniverseKit.HydraTextures."
    f"{FOLLOW_CAMERA_RENDER_PRODUCT_NAME}.LdrColor"
)


def livestream_aov_arguments(
    *,
    signal_port: int,
    media_port: int,
    public_ip: str,
    target_fps: int,
) -> list[str]:
    settings = (
        ("streamType", "webrtc"),
        ("signalPort", str(signal_port)),
        ("streamPort", str(media_port)),
        ("publicIp", public_ip),
        ("targetFps", str(target_fps)),
        ("allowDynamicResize", "false"),
    )
    prefix = (
        f"--/exts/omni.kit.livestream.aov/{FOLLOW_CAMERA_LDR_COLOR_AOV}/"
        "spectatorStream/0"
    )
    return [f"{prefix}/{name}={value}" for name, value in settings]
