"""Observation-frame extraction from the server's remuxed MP4.

The extraction contract writes MP4 with a 1 GHz media timescale and no
B-frames, so presentation order equals decode order and presentation time in
nanoseconds plus `decode_start_index` reconstructs the original Rerun index.
"""

from __future__ import annotations

from dataclasses import dataclass
from fractions import Fraction
from pathlib import Path

import av
from PIL import Image


@dataclass(frozen=True)
class ObservedFrame:
    """One observation frame with its original recording timeline index."""

    index: int
    image: Image.Image


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
) -> list[ObservedFrame]:
    with av.open(str(input_mp4)) as container:
        stream = container.streams.video[0]
        total = sum(1 for packet in container.demux(stream) if packet.pts is not None)
    selected = set(uniform_indices(total, max_frames))
    frames: list[ObservedFrame] = []
    with av.open(str(input_mp4)) as container:
        stream = container.streams.video[0]
        time_base = Fraction(stream.time_base)
        position = 0
        for frame in container.decode(stream):
            if position in selected and frame.pts is not None:
                image = frame.to_image().resize(
                    (observation_width, observation_height), Image.BILINEAR
                )
                frames.append(
                    ObservedFrame(
                        index=frame_index(int(frame.pts), time_base, decode_start_index),
                        image=image,
                    )
                )
            position += 1
    if not frames:
        raise ValueError(f"no decodable frames in {input_mp4}")
    return frames
