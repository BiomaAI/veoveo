"""Attach Isaac Sim's NVENC RTSP writer to an existing RTX render product."""

from __future__ import annotations

from typing import TYPE_CHECKING
from dataclasses import dataclass

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

    writer: RTSPStreamWriter | None = None
    try:
        with Usd.EditContext(stage, stage.GetSessionLayer()):
            created, _ = ensure_render_var_on_product(
                stage, render_product_path, "LdrColor", "h264"
            )
            if not created:
                raise RuntimeError(
                    f"Isaac RTSP writer could not attach LdrColor to {render_product_path}"
                )
            writer = RTSPStreamWriter(
                port=port,
                mountPath="/stream",
                # SRTX owns the one NVENC encode and the stock writer carries
                # render-reference simulation time in each frame's SEI.
                encoding="h264",
                width=width,
                height=height,
            )
            writer.attach([render_product_path])
    except BaseException as original:
        # Retain acquisition through attach AND edit-context exit.
        if writer is not None:
            owner = _AttachedWriterRetirement(writer)
            try:
                owner.close()
            except BaseException:
                from .hydra_camera import RetainedProductAcquisitionError
                raise RetainedProductAcquisitionError(owner) from original
        raise
    assert writer is not None
    return writer


class _AttachedWriterRetirement:
    def __init__(self, writer: RTSPStreamWriter) -> None:
        self._writer: RTSPStreamWriter | None = writer
        self._failure: BaseException | None = None

    @property
    def cleanup_complete(self) -> bool:
        return self._writer is None

    def close(self) -> None:
        if self._writer is not None:
            try:
                self._writer.detach()
            except BaseException as error:
                if self._failure is None:
                    self._failure = error
            else:
                self._writer = None
        if self._failure is not None:
            raise self._failure


# Stock Isaac Sim 6.1 RTSPStreamWriter user_data_unregistered metadata UUID.
_CAPTURE_UUID = bytes.fromhex("aa71e48f07115d80a247cd31ca6fa49c")


@dataclass(frozen=True, slots=True)
class CaptureMetadata:
    publish_sim_time_ns: int
    timestamp_iso8601: str
    timestamp: int
    frame_num: int

    @classmethod
    def admit(cls, value: object) -> CaptureMetadata:
        if not isinstance(value, dict) or set(value) != {
            "publish_sim_time_ns", "timestamp_iso8601", "timestamp", "frame_num"
        }:
            raise RuntimeError("native capture metadata has an unsupported shape")
        time_ns = value["publish_sim_time_ns"]
        frame = value["frame_num"]
        timestamp = value["timestamp"]
        iso = value["timestamp_iso8601"]
        if (type(time_ns) is not int or time_ns < 0
            or type(frame) is not int or frame < 1
            or type(timestamp) is not int or timestamp < 0
            or not isinstance(iso, str) or not iso or len(iso) > 128):
            raise RuntimeError("native capture metadata has an invalid clock or frame")
        import re
        from datetime import datetime
        if not re.fullmatch(r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{1,9})?(?:Z|[+-]\d{2}:\d{2})", iso):
            raise RuntimeError("native capture metadata has an invalid anchor")
        try:
            datetime.fromisoformat(iso)
        except ValueError as error:
            raise RuntimeError("native capture metadata has an invalid anchor") from error
        return cls(time_ns, iso, timestamp, frame)


def capture_simulation_time(sample: bytes) -> CaptureMetadata:
    """Admit the stock per-picture capture clock without altering the bitstream."""
    import json
    import re

    if not sample or len(sample) > 16 * 1024 * 1024:
        raise RuntimeError("native capture access unit exceeds its size bound")

    def sei_nals():
        # Views keep the 64 KiB check ahead of RBSP/payload allocations.
        previous = None
        nal_count = 0
        for marker in re.finditer(b"\x00\x00(?:\x00)?\x01", sample):
            nal_count += 1
            if nal_count > 4096:
                raise RuntimeError("native capture access unit exceeds its NAL bound")
            if previous is not None and previous < marker.start():
                if sample[previous] & 0x1F == 6:
                    yield memoryview(sample)[previous:marker.start()]
            previous = marker.end()
        if previous is not None and previous < len(sample) and sample[previous] & 0x1F == 6:
            yield memoryview(sample)[previous:]

    found: CaptureMetadata | None = None
    for nal in sei_nals():
        if nal[0] & 0x1F != 6:
            continue
        if len(nal) > 65536:
            raise RuntimeError("native capture SEI NAL exceeds its size bound")
        # Annex B permits trailing_zero_8bits outside the RBSP.
        end = len(nal)
        while end > 1 and nal[end - 1] == 0:
            end -= 1
        nal = nal[:end]
        rbsp = bytearray()
        zeros = 0
        for index, byte in enumerate(nal[1:], start=1):
            if zeros >= 2:
                if byte == 3:
                    if index + 1 >= len(nal) or nal[index + 1] > 3:
                        raise RuntimeError("native capture SEI has invalid emulation prevention")
                    zeros = 0
                    continue
                if byte < 3:
                    raise RuntimeError("native capture SEI has unescaped start code")
            rbsp.append(byte)
            zeros = zeros + 1 if byte == 0 else 0
        cursor = 0
        messages = 0
        while cursor < len(rbsp) and rbsp[cursor] != 0x80:
            messages += 1
            if messages > 256:
                raise RuntimeError("native capture SEI exceeds its message bound")
            payload_type = 0
            while cursor < len(rbsp) and rbsp[cursor] == 255:
                payload_type += 255
                cursor += 1
            if cursor >= len(rbsp):
                raise RuntimeError("native capture SEI type is truncated")
            payload_type += rbsp[cursor]
            cursor += 1
            length = 0
            while cursor < len(rbsp) and rbsp[cursor] == 255:
                length += 255
                cursor += 1
            if cursor >= len(rbsp):
                raise RuntimeError("native capture SEI length is truncated")
            length += rbsp[cursor]
            cursor += 1
            end = cursor + length
            if end > len(rbsp):
                raise RuntimeError("native capture SEI payload is truncated")
            payload = memoryview(rbsp)[cursor:end]
            cursor = end
            if payload_type == 5 and len(payload) < 16:
                raise RuntimeError("native capture SEI UUID is truncated")
            if payload_type != 5 or payload[:16] != _CAPTURE_UUID:
                continue
            if found is not None:
                raise RuntimeError("native picture has repeated capture metadata")
            if len(payload) - 16 > 4096:
                raise RuntimeError("native capture metadata exceeds its size bound")
            def unique_fields(pairs: list[tuple[str, object]]) -> dict[str, object]:
                result: dict[str, object] = {}
                for key, value in pairs:
                    if key in result:
                        raise ValueError("repeated metadata field")
                    result[key] = value
                return result
            try:
                facts = json.loads(bytes(payload[16:]).decode("utf-8"), object_pairs_hook=unique_fields)
            except (UnicodeError, ValueError) as error:
                raise RuntimeError("native capture metadata is malformed") from error
            found = CaptureMetadata.admit(facts)
        if cursor >= len(rbsp) or rbsp[cursor] != 0x80 or any(rbsp[cursor + 1:]):
            raise RuntimeError("native capture SEI has invalid trailing bits")
    if found is None:
        raise RuntimeError("native picture is missing stock capture metadata")
    return found
