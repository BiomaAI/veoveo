"""Regenerate CUDA sensor data from the pinned PX4 magnetic-model header.

Usage: python generate_magnetic_tables.py <px4-source-directory>
The source checksum binds this adapter to the same model as the pinned estimator.
"""
from __future__ import annotations

import hashlib
from pathlib import Path
import re
import sys


PX4_COMMIT = "d6f12ad1c4f70ad3230afd7d86e971421e02fef4"
HEADER_SHA256 = "3e985cba6a1fd9b246d628bf1d24b843b306a5a6d60ec69900188ec535a3b8cb"


def generate(source: Path) -> str:
    content = source.read_bytes()
    if hashlib.sha256(content).hexdigest() != HEADER_SHA256:
        raise ValueError("magnetic tables differ from the qualified PX4 source")
    header = content.decode()
    license_text = header.split("#include", 1)[0]
    lines = [
        '"""Generated PX4 WMM-2020 tables, epoch 2024.41257; do not edit.',
        f"Source: PX4/PX4-Autopilot {PX4_COMMIT}",
        f"Header SHA-256: {HEADER_SHA256}",
        "Regenerate with runtime/generate_magnetic_tables.py.",
        "", license_text.rstrip(), '"""', "",
    ]
    for name, scale in (
        ("declination", "WMM_DECLINATION_SCALE_TO_DEGREES"),
        ("inclination", "WMM_INCLINATION_SCALE_TO_DEGREES"),
        ("totalintensity", "WMM_TOTALINTENSITY_SCALE_TO_NANOTESLA"),
    ):
        body = re.search(rf"{name}_table\[19\]\[37\] \{{(.*?)\n\}};", header, re.S)
        factor = re.search(rf"{scale} = ([\d.]+)f;", header)
        if body is None or factor is None:
            raise ValueError(f"missing PX4 magnetic table: {name}")
        rows = re.findall(r"\{([^{}]+)\}", body[1])
        parsed = [tuple(int(value) for value in row.split(",") if value.strip()) for row in rows]
        if len(parsed) != 19 or any(len(row) != 37 for row in parsed):
            raise ValueError(f"invalid PX4 magnetic table dimensions: {name}")
        lines.extend((f"{scale} = {factor[1]}", f"{name.upper()} = ("))
        lines.extend(f"    {row}," for row in parsed)
        lines.extend((")", ""))
    return "\n".join(lines)


if __name__ == "__main__":
    source = Path(sys.argv[1]) / "src/lib/world_magnetic_model/geo_magnetic_tables.hpp"
    target = Path(__file__).parent / "veoveo_uav_sim/_magnetic_tables.py"
    target.write_text(generate(source))
