from __future__ import annotations

from pathlib import Path
import json
import os
import shutil

from map_data.adapters.common import write_quality_report
from map_data.contract import NormalizeCommand
from map_data.subprocesses import run_tool


def normalize_osm(command: NormalizeCommand) -> tuple[tuple[Path, ...], Path, Path | None]:
    run_tool(
        "osmium",
        ["check-refs", str(command.source_path)],
        timeout_seconds=command.maximum_elapsed_seconds,
        cwd=command.output_dir,
    )
    parquet = command.output_dir / "osm-features.parquet"
    run_tool(
        "ogr2ogr",
        [
            "-f", "Parquet", "-t_srs", "EPSG:4326", "-skipfailures",
            str(parquet), str(command.source_path), "points", "-where", "name IS NOT NULL",
        ],
        timeout_seconds=command.maximum_elapsed_seconds,
        cwd=command.output_dir,
    )
    places = command.output_dir / "osm-places.geojsonseq"
    run_tool(
        "ogr2ogr",
        [
            "-f", "GeoJSONSeq", "-t_srs", "EPSG:4326", "-skipfailures",
            str(places), str(command.source_path), "points", "-where", "name IS NOT NULL",
        ],
        timeout_seconds=command.maximum_elapsed_seconds,
        cwd=command.output_dir,
    )
    routing_directory = command.output_dir / "valhalla"
    routing_directory.mkdir()
    valhalla_config = os.environ.get("MAP_VALHALLA_BUILD_CONFIG")
    if not valhalla_config:
        raise RuntimeError("MAP_VALHALLA_BUILD_CONFIG is required for OpenStreetMap acquisition")
    config = json.loads(Path(valhalla_config).read_text(encoding="utf-8"))
    mjolnir = config.get("mjolnir")
    if not isinstance(mjolnir, dict):
        raise RuntimeError("Valhalla build configuration has no mjolnir object")
    mjolnir["tile_dir"] = str(routing_directory)
    mjolnir["tile_extract"] = str(routing_directory / "tiles.tar")
    mjolnir["admin"] = str(routing_directory / "admins.sqlite")
    mjolnir["timezone"] = str(routing_directory / "timezones.sqlite")
    build_config = command.output_dir / "valhalla-build.json"
    build_config.write_text(json.dumps(config), encoding="utf-8")
    run_tool(
        "valhalla_build_tiles",
        ["-c", str(build_config), str(command.source_path)],
        timeout_seconds=command.maximum_elapsed_seconds,
        cwd=routing_directory,
    )
    routing = Path(
        shutil.make_archive(
            str(command.output_dir / "valhalla-tiles"),
            "gztar",
            root_dir=routing_directory,
        )
    )
    report = write_quality_report(
        command,
        adapter="open_street_map",
        checks=[
            {"name": "osmium_check_refs", "passed": True},
            {"name": "geoparquet_created", "passed": parquet.is_file()},
            {"name": "routing_build_created", "passed": routing is not None},
            {"name": "named_places_created", "passed": places.is_file()},
        ],
    )
    return (parquet, places), report, routing
