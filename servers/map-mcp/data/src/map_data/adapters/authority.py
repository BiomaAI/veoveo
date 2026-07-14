from __future__ import annotations

from pathlib import Path

from map_data.adapters.common import write_quality_report
from map_data.contract import NormalizeCommand
from map_data.subprocesses import run_tool


def normalize_authority(command: NormalizeCommand) -> tuple[tuple[Path, ...], Path, Path | None]:
    parquet = command.output_dir / "authority-features.parquet"
    geojson = command.output_dir / "authority-features.geojson"
    run_tool(
        "ogr2ogr",
        ["-f", "Parquet", str(parquet), str(command.source_path)],
        timeout_seconds=command.maximum_elapsed_seconds,
        cwd=command.output_dir,
    )
    run_tool(
        "ogr2ogr",
        ["-f", "GeoJSON", "-t_srs", "EPSG:4326", str(geojson), str(command.source_path)],
        timeout_seconds=command.maximum_elapsed_seconds,
        cwd=command.output_dir,
    )
    report = write_quality_report(
        command,
        adapter="authority_vector",
        checks=[
            {"name": "geoparquet_created", "passed": parquet.is_file()},
            {"name": "wgs84_exchange_created", "passed": geojson.is_file()},
        ],
    )
    return (parquet, geojson), report, None
