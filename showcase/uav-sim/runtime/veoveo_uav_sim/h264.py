from __future__ import annotations

from dataclasses import dataclass


_SEQUENCE_PARAMETER_SET = 7
_PICTURE_PARAMETER_SET = 8
_IDR_PICTURE = 5


@dataclass(frozen=True, slots=True)
class NativeH264AccessUnit:
    """One decoder-reentrant Annex B frame emitted by Isaac's NVENC node."""

    sample: bytes
    nal_types: tuple[int, ...]

    @property
    def is_keyframe(self) -> bool:
        return _IDR_PICTURE in self.nal_types


def parse_native_h264_access_unit(sample: bytes) -> NativeH264AccessUnit:
    """Validate Isaac's one-IDR-per-frame native recording profile."""
    nals = annex_b_nals(sample)
    nal_types = tuple(nal[0] & 0x1F for nal in nals)
    missing = [
        name
        for nal_type, name in (
            (_SEQUENCE_PARAMETER_SET, "SPS"),
            (_PICTURE_PARAMETER_SET, "PPS"),
            (_IDR_PICTURE, "IDR"),
        )
        if nal_type not in nal_types
    ]
    if missing:
        raise ValueError(
            "native Isaac H.264 access unit is not decoder-reentrant; missing "
            + ", ".join(missing)
        )
    return NativeH264AccessUnit(bytes(sample), nal_types)


def annex_b_nals(sample: bytes) -> tuple[bytes, ...]:
    starts: list[tuple[int, int]] = []
    cursor = 0
    while cursor + 3 <= len(sample):
        if sample[cursor : cursor + 4] == b"\x00\x00\x00\x01":
            starts.append((cursor, 4))
            cursor += 4
        elif sample[cursor : cursor + 3] == b"\x00\x00\x01":
            starts.append((cursor, 3))
            cursor += 3
        else:
            cursor += 1
    if not starts:
        raise ValueError("native Isaac H.264 sample is not Annex B")
    output: list[bytes] = []
    for index, (start, prefix_length) in enumerate(starts):
        end = starts[index + 1][0] if index + 1 < len(starts) else len(sample)
        while end > start + prefix_length and sample[end - 1] == 0:
            end -= 1
        nal = sample[start + prefix_length : end]
        if not nal:
            raise ValueError("native Isaac H.264 access unit contains an empty NAL")
        output.append(nal)
    return tuple(output)
