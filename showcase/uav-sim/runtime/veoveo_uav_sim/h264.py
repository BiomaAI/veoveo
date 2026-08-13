from __future__ import annotations

from dataclasses import dataclass

_SEQUENCE_PARAMETER_SET = 7
_PICTURE_PARAMETER_SET = 8
_IDR_PICTURE = 5


@dataclass(frozen=True, slots=True)
class NativeH264AccessUnit:
    """One Annex B access unit emitted by Isaac's native NVENC stream."""

    sample: bytes
    nal_types: tuple[int, ...]

    @property
    def is_keyframe(self) -> bool:
        return _IDR_PICTURE in self.nal_types

    @property
    def is_decoder_reentrant(self) -> bool:
        return all(
            nal_type in self.nal_types
            for nal_type in (
                _SEQUENCE_PARAMETER_SET,
                _PICTURE_PARAMETER_SET,
                _IDR_PICTURE,
            )
        )

    @property
    def parameter_sets(self) -> tuple[bytes | None, bytes | None]:
        sequence: bytes | None = None
        picture: bytes | None = None
        for nal in annex_b_nals(self.sample):
            nal_type = nal[0] & 0x1F
            if nal_type == _SEQUENCE_PARAMETER_SET:
                sequence = nal
            elif nal_type == _PICTURE_PARAMETER_SET:
                picture = nal
        return sequence, picture


def parse_native_h264_access_unit(sample: bytes) -> NativeH264AccessUnit:
    """Validate one complete native H.264 Annex B access unit."""
    nals = annex_b_nals(sample)
    nal_types = tuple(nal[0] & 0x1F for nal in nals)
    if not any(1 <= nal_type <= 5 for nal_type in nal_types):
        raise ValueError("native Isaac H.264 access unit contains no coded picture")
    return NativeH264AccessUnit(bytes(sample), nal_types)


def make_decoder_reentrant(
    access_unit: NativeH264AccessUnit,
    sequence_parameter_set: bytes,
    picture_parameter_set: bytes,
) -> NativeH264AccessUnit:
    """Prepend missing codec state to an IDR without re-encoding it."""
    if not access_unit.is_keyframe:
        raise ValueError("only an IDR access unit can become decoder-reentrant")
    if not sequence_parameter_set or sequence_parameter_set[0] & 0x1F != 7:
        raise ValueError("invalid H.264 sequence parameter set")
    if not picture_parameter_set or picture_parameter_set[0] & 0x1F != 8:
        raise ValueError("invalid H.264 picture parameter set")
    prefix = bytearray()
    if _SEQUENCE_PARAMETER_SET not in access_unit.nal_types:
        prefix.extend(b"\x00\x00\x00\x01")
        prefix.extend(sequence_parameter_set)
    if _PICTURE_PARAMETER_SET not in access_unit.nal_types:
        prefix.extend(b"\x00\x00\x00\x01")
        prefix.extend(picture_parameter_set)
    if not prefix:
        return access_unit
    return parse_native_h264_access_unit(bytes(prefix) + access_unit.sample)


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
