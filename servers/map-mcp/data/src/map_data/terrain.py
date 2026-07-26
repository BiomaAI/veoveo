from __future__ import annotations

import math

from map_data.contract import ContractError


def corridor_sample_positions(
    positions: list[tuple[float, float]],
    spacing: float,
    half_width: float,
    cross_track_samples: int,
) -> list[tuple[float, float, float, float]]:
    radius = 6_371_008.8
    longitude_origin = sum(position[0] for position in positions) / len(positions)
    latitude_origin = sum(position[1] for position in positions) / len(positions)
    if (
        max(position[0] for position in positions)
        - min(position[0] for position in positions)
        > 2
        or max(position[1] for position in positions)
        - min(position[1] for position in positions)
        > 2
        or abs(latitude_origin) > 85
    ):
        raise ContractError("corridor exceeds the local two-degree sampling envelope")
    longitude_radians = math.radians(longitude_origin)
    latitude_radians = math.radians(latitude_origin)
    cosine_latitude = math.cos(latitude_radians)

    def project(position: tuple[float, float]) -> tuple[float, float]:
        return (
            radius
            * (math.radians(position[0]) - longitude_radians)
            * cosine_latitude,
            radius * (math.radians(position[1]) - latitude_radians),
        )

    def unproject(position: tuple[float, float]) -> tuple[float, float]:
        return (
            math.degrees(
                longitude_radians + position[0] / (radius * cosine_latitude)
            ),
            math.degrees(latitude_radians + position[1] / radius),
        )

    line = [project(position) for position in positions]
    segment_lengths = [
        math.hypot(second[0] - first[0], second[1] - first[1])
        for first, second in zip(line, line[1:])
    ]
    total_length = sum(segment_lengths)
    if total_length <= 0:
        raise ContractError("corridor has zero length")
    along_targets = [
        min(index * spacing, total_length)
        for index in range(math.ceil(total_length / spacing) + 1)
    ]
    if along_targets[-1] != total_length:
        along_targets.append(total_length)
    cross_offsets = (
        [0.0]
        if cross_track_samples == 1
        else [
            -half_width + 2 * half_width * index / (cross_track_samples - 1)
            for index in range(cross_track_samples)
        ]
    )
    output = []
    segment_index = 0
    segment_start = 0.0
    for target in along_targets:
        while (
            segment_index + 1 < len(segment_lengths)
            and target > segment_start + segment_lengths[segment_index]
        ):
            segment_start += segment_lengths[segment_index]
            segment_index += 1
        length = segment_lengths[segment_index]
        if length == 0:
            continue
        fraction = min(1.0, max(0.0, (target - segment_start) / length))
        first = line[segment_index]
        second = line[segment_index + 1]
        center = (
            first[0] + (second[0] - first[0]) * fraction,
            first[1] + (second[1] - first[1]) * fraction,
        )
        tangent = (
            (second[0] - first[0]) / length,
            (second[1] - first[1]) / length,
        )
        normal = (-tangent[1], tangent[0])
        for offset in cross_offsets:
            longitude, latitude = unproject(
                (center[0] + normal[0] * offset, center[1] + normal[1] * offset)
            )
            output.append((target, offset, longitude, latitude))
    return output
