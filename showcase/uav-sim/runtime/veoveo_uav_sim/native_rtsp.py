"""Attach Isaac Sim's NVENC RTSP writer to an existing RTX render product."""

from __future__ import annotations

from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from isaacsim.streaming.rtsp import RTSPStreamWriter


def attach_native_rtsp_writer(
    render_product_path: str,
    *,
    port: int,
    width: int,
    height: int,
) -> RTSPStreamWriter:
    import omni.usd
    from isaacsim.streaming.rtsp import RTSPStreamWriter
    from isaacsim.streaming.rtsp.impl.render_var_utils import (
        ensure_render_var_on_product,
    )
    from pxr import Usd

    stage = omni.usd.get_context().get_stage()
    if stage is None:
        raise RuntimeError("Isaac RTSP writer requires an active USD stage")

    with Usd.EditContext(stage, stage.GetSessionLayer()):
        created, _ = ensure_render_var_on_product(
            stage, render_product_path, "LdrColor", ""
        )
        if not created:
            raise RuntimeError(
                f"Isaac RTSP writer could not attach LdrColor to {render_product_path}"
            )
        writer = RTSPStreamWriter(
            port=port,
            mountPath="/stream",
            # NVIDIA's writer passes the resident CUDA buffer directly to its
            # RTSP backend, which owns the one NVENC encode for this product.
            encoding="raw",
            width=width,
            height=height,
        )
        writer.attach([render_product_path])
    return writer
