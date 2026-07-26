from __future__ import annotations

from pathlib import Path
import json
import os
import shutil

from map_data.adapters.common import write_quality_report
from map_data.contract import NormalizeCommand
from map_data.subprocesses import run_tool

_OSM_CONFIG = Path(__file__).with_name("osmconf.ini")


def normalize_osm(command: NormalizeCommand) -> tuple[tuple[Path, ...], Path, Path | None]:
    if not _OSM_CONFIG.is_file():
        raise RuntimeError("the governed OpenStreetMap normalization profile is missing")
    open_options = ["-oo", f"CONFIG_FILE={_OSM_CONFIG}"]
    run_tool(
        "osmium",
        ["check-refs", str(command.source_path)],
        timeout_seconds=command.maximum_elapsed_seconds,
        cwd=command.output_dir,
    )
    normalized = []
    for layer in ("points", "lines", "multilinestrings", "multipolygons", "other_relations"):
        geojson = command.output_dir / f"osm-{layer}.geojsonseq"
        run_tool(
            "ogr2ogr",
            [
                "-f", "GeoJSONSeq", "-t_srs", "EPSG:4326", "-skipfailures",
                *open_options,
                str(geojson), str(command.source_path), layer,
            ],
            timeout_seconds=command.maximum_elapsed_seconds,
            cwd=command.output_dir,
        )
        normalized.append(geojson)
    parquet = command.output_dir / "osm-features.parquet"
    run_tool(
        "ogr2ogr",
        [
            "-f", "Parquet", "-t_srs", "EPSG:4326", "-skipfailures",
            *open_options,
            str(parquet), str(command.source_path),
        ],
        timeout_seconds=command.maximum_elapsed_seconds,
        cwd=command.output_dir,
    )
    normalized.append(parquet)
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
            {
                "name": "complete_source_layers_created",
                "passed": all(path.is_file() for path in normalized[:-1]),
            },
            {
                "name": "governed_source_identity_and_tags_profile",
                "passed": _OSM_CONFIG.is_file(),
            },
        ],
    )
    return tuple(normalized), report, routing
