"""Observation-frame extraction from the server's remuxed MP4.

The extraction contract writes MP4 with a 1 GHz media timescale and no
B-frames, so presentation order equals decode order and presentation time in
nanoseconds plus `decode_start_index` reconstructs the original Rerun index.
"""

from __future__ import annotations

from dataclasses import dataclass
from fractions import Fraction
from pathlib import Path
from typing import TYPE_CHECKING

import av

if TYPE_CHECKING:
    import torch


@dataclass(frozen=True)
class ObservedFrame:
    """One observation frame with its original recording timeline index."""

    index: int
    image: torch.Tensor


def uniform_indices(total: int, maximum: int) -> list[int]:
    """Evenly spaced positions covering `total` frames with at most `maximum`."""
    if total <= 0 or maximum <= 0:
        return []
    if total <= maximum:
        return list(range(total))
    if maximum == 1:
        return [0]
    return sorted({round(position * (total - 1) / (maximum - 1)) for position in range(maximum)})


def frame_index(pts: int, time_base: Fraction, decode_start_index: int) -> int:
    nanoseconds = pts * time_base.numerator * 1_000_000_000 // time_base.denominator
    return decode_start_index + nanoseconds


def sample_frames(
    input_mp4: Path,
    max_frames: int,
    observation_width: int,
    observation_height: int,
    decode_start_index: int,
    input_width: int,
    input_height: int,
) -> list[ObservedFrame]:
    import PyNvVideoCodec as nvc
    import torch
    from torchvision.transforms.v2 import functional as transforms
    from torchvision.transforms import InterpolationMode

    if not torch.cuda.is_available():
        raise RuntimeError("Reason frame extraction requires an NVIDIA CUDA device")
    if min(max_frames, observation_width, observation_height, input_width, input_height) <= 0:
        raise ValueError("frame counts and dimensions must be positive")
    # Demuxing reads container metadata only. Do not open a software decoder:
    # exact packet PTS, rather than an estimated frame rate, retains Rerun time.
    with av.open(str(input_mp4)) as container:
        if len(container.streams.video) != 1:
            raise ValueError("Reason input must contain exactly one video stream")
        stream = container.streams.video[0]
        if (stream.width, stream.height) != (input_width, input_height):
            raise ValueError("input dimensions differ from the bounded runner request")
        time_base = Fraction(stream.time_base)
        timestamps = [int(packet.pts) for packet in container.demux(stream) if packet.pts is not None]
    if not timestamps or any(a >= b for a, b in zip(timestamps, timestamps[1:])):
        raise ValueError("Reason requires increasing presentation timestamps without B-frames")
    selected = uniform_indices(len(timestamps), max_frames)
    frames: list[ObservedFrame] = []
    cuda_stream = torch.cuda.current_stream()
    decoder = nvc.SimpleDecoder(
        str(input_mp4),
        output_color_type=nvc.OutputColorType.RGB,
        use_device_memory=True,
        need_scanned_stream_metadata=True,
        gpu_id=torch.cuda.current_device(),
        cuda_stream=cuda_stream.cuda_stream,
        decoder_cache_size=1,
    )
    try:
        # One surface at a time bounds source-resolution retention. Each resized
        # tensor owns its CUDA allocation before the decoder can reuse a surface.
        for position in selected:
            decoded = decoder.get_batch_frames_by_index([position])
            if len(decoded) != 1:
                raise ValueError(f"NVDEC did not return selected frame {position}")
            tensor = torch.from_dlpack(decoded[0])
            if not tensor.is_cuda or tensor.dtype != torch.uint8:
                raise RuntimeError("NVDEC must return CUDA uint8 RGB surfaces")
            if tuple(tensor.shape) == (input_height, input_width, 3):
                tensor = tensor.permute(2, 0, 1)
            if tuple(tensor.shape) != (3, input_height, input_width):
                raise ValueError("NVDEC surface shape differs from the input contract")
            resized = transforms.resize(
                tensor,
                [observation_height, observation_width],
                interpolation=InterpolationMode.BILINEAR,
                antialias=True,
            ).clone()
            frames.append(ObservedFrame(
                index=frame_index(timestamps[position], time_base, decode_start_index),
                image=resized,
            ))
            # DLPack owns the decoded view until its consuming CUDA work ends.
            # This stream wait does not read image data back to the CPU.
            cuda_stream.synchronize()
            del tensor, decoded
    finally:
        cuda_stream.synchronize()
        del decoder
    return frames
