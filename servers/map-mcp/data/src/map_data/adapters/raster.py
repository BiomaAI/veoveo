from __future__ import annotations

import hashlib
import json
import math
from pathlib import Path
from typing import Any

from map_data.adapters.common import write_quality_report
from map_data.contract import ContractError, NormalizeCommand
from map_data.subprocesses import run_tool


def normalize_environmental(
    command: NormalizeCommand,
) -> tuple[tuple[Path, ...], Path, Path | None]:
    raster = command.output_dir / "environmental-raster.tif"
    run_tool(
        "gdal_translate",
        [
            "-of",
            "COG",
            "-co",
            "COMPRESS=ZSTD",
            "-co",
            "OVERVIEWS=AUTO",
            str(command.source_path),
            str(raster),
        ],
        timeout_seconds=command.maximum_elapsed_seconds,
        cwd=command.output_dir,
    )
    completed = run_tool(
        "gdalinfo",
        ["-json", str(raster)],
        timeout_seconds=command.maximum_elapsed_seconds,
        cwd=command.output_dir,
    )
    info = json.loads(completed.stdout)
    metadata = raster_metadata(info, raster)
    metadata_path = command.output_dir / "environmental-raster.raster.json"
    metadata_path.write_text(
        json.dumps(metadata, separators=(",", ":"), sort_keys=True),
        encoding="utf-8",
    )
    report = write_quality_report(
        command,
        adapter="environmental",
        checks=[
            {
                "name": "cloud_optimized_geotiff_created",
                "passed": raster.is_file() and is_cloud_optimized_geotiff(info),
            },
            {
                "name": "complete_raster_metadata_created",
                "passed": metadata_path.is_file(),
            },
        ],
    )
    return (raster, metadata_path), report, None


def raster_metadata(info: Any, raster: Path) -> dict[str, Any]:
    if not isinstance(info, dict):
        raise ContractError("gdalinfo returned a non-object")
    size = info.get("size")
    transform = info.get("geoTransform")
    bands = info.get("bands")
    corners = info.get("cornerCoordinates")
    coordinate_system = info.get("coordinateSystem")
    if (
        not is_cloud_optimized_geotiff(info)
        or not isinstance(size, list)
        or len(size) != 2
        or not all(isinstance(value, int) and value > 0 for value in size)
        or not isinstance(transform, list)
        or len(transform) != 6
        or not all(isinstance(value, (int, float)) for value in transform)
        or not isinstance(bands, list)
        or not bands
        or not isinstance(corners, dict)
        or not isinstance(coordinate_system, dict)
    ):
        raise ContractError("gdalinfo omitted required raster metadata")
    corner_values = [
        corners.get("upperLeft"),
        corners.get("lowerLeft"),
        corners.get("lowerRight"),
        corners.get("upperRight"),
    ]
    if any(
        not isinstance(corner, list)
        or len(corner) < 2
        or not all(
            isinstance(coordinate, (int, float)) and math.isfinite(coordinate)
            for coordinate in corner[:2]
        )
        for corner in corner_values
    ):
        raise ContractError("gdalinfo omitted the raster extent")
    xs = [float(corner[0]) for corner in corner_values]
    ys = [float(corner[1]) for corner in corner_values]
    crs = coordinate_system.get("wkt")
    if not isinstance(crs, str) or not crs:
        raise ContractError("gdalinfo omitted the raster CRS")
    crs = " ".join(crs.split())
    if not crs:
        raise ContractError("gdalinfo returned an empty raster CRS")
    return {
        "schema_version": 1,
        "source_file": raster.name,
        "checksum_sha256": sha256(raster),
        "crs": crs,
        "transform": transform,
        "width": size[0],
        "height": size[1],
        "extent": [min(xs), min(ys), max(xs), max(ys)],
        "resolution": [
            math.hypot(transform[1], transform[2]),
            math.hypot(transform[4], transform[5]),
        ],
        "bands": [raster_band(index, band) for index, band in enumerate(bands, start=1)],
    }


def raster_band(index: int, value: Any) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise ContractError("gdalinfo returned an invalid band")
    data_type = value.get("type")
    if not isinstance(data_type, str) or not data_type:
        raise ContractError("gdalinfo band omitted its data type")
    name = value.get("description")
    unit = value.get("unit")
    nodata = value.get("noDataValue")
    if name is not None and not isinstance(name, str):
        raise ContractError("gdalinfo returned an invalid band description")
    if unit is not None and not isinstance(unit, str):
        raise ContractError("gdalinfo returned an invalid band unit")
    if nodata is not None and (
        not isinstance(nodata, (int, float))
        or isinstance(nodata, bool)
        or not math.isfinite(nodata)
    ):
        raise ContractError("gdalinfo returned an invalid band nodata value")
    color = value.get("colorInterpretation")
    interpretation = explicit_value_interpretation(value) or (
        "color"
        if color in {"Red", "Green", "Blue", "Alpha"}
        else "categorical"
        if color in {"Category", "PaletteIndex"}
        else "continuous"
    )
    return {
        "index": index,
        "name": name or None,
        "data_type": data_type,
        "unit": unit or None,
        "interpretation": interpretation,
        "nodata": nodata,
    }


def is_cloud_optimized_geotiff(info: Any) -> bool:
    if not isinstance(info, dict) or info.get("driverShortName") != "GTiff":
        return False
    metadata = info.get("metadata")
    return (
        isinstance(metadata, dict)
        and isinstance(metadata.get("IMAGE_STRUCTURE"), dict)
        and metadata["IMAGE_STRUCTURE"].get("LAYOUT") == "COG"
    )


def explicit_value_interpretation(value: dict[str, Any]) -> str | None:
    metadata = value.get("metadata")
    if not isinstance(metadata, dict):
        return None
    allowed = {
        "continuous",
        "categorical",
        "probability",
        "vector_component",
        "color",
        "mask",
    }
    for domain in metadata.values():
        if not isinstance(domain, dict):
            continue
        interpretation = domain.get("VEOVEO_VALUE_INTERPRETATION")
        if interpretation is None:
            continue
        if not isinstance(interpretation, str) or interpretation not in allowed:
            raise ContractError("raster band declares an invalid value interpretation")
        return interpretation
    return None


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        while chunk := source.read(1024 * 1024):
            digest.update(chunk)
    return digest.hexdigest()
