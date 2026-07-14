from __future__ import annotations

from pathlib import Path
import os
import zipfile

from map_data.adapters.common import copy_as_normalized, write_quality_report
from map_data.contract import ContractError, NormalizeCommand
from map_data.subprocesses import run_tool

REQUIRED_FILES = {"agency.txt", "routes.txt", "stops.txt", "trips.txt", "stop_times.txt"}


def normalize_gtfs(command: NormalizeCommand) -> tuple[tuple[Path, ...], Path, Path | None]:
    with zipfile.ZipFile(command.source_path) as archive:
        entries = [name for name in archive.namelist() if not name.endswith("/")]
        if len(entries) > 10_000:
            raise ContractError("GTFS archive contains too many entries")
        for name in entries:
            path = Path(name)
            if path.is_absolute() or ".." in path.parts or "\\" in name:
                raise ContractError("GTFS archive contains an unsafe entry path")
        expanded_bytes = sum(info.file_size for info in archive.infolist())
        compressed_bytes = max(1, sum(info.compress_size for info in archive.infolist()))
        if expanded_bytes > command.maximum_output_bytes or expanded_bytes / compressed_bytes > 100:
            raise ContractError("GTFS archive exceeds expansion limits")
        names = {Path(name).name for name in entries}
        missing = REQUIRED_FILES - names
        if missing:
            raise ContractError(f"GTFS archive is missing required files: {sorted(missing)}")
    validator = os.environ.get("MAP_GTFS_VALIDATOR_JAR")
    if validator:
        run_tool(
            "java",
            ["-jar", validator, "-i", str(command.source_path), "-o", str(command.output_dir / "gtfs-validator")],
            timeout_seconds=command.maximum_elapsed_seconds,
            cwd=command.output_dir,
        )
    normalized = copy_as_normalized(command, ".gtfs.zip")
    report = write_quality_report(
        command,
        adapter="gtfs_schedule",
        checks=[
            {"name": "required_files_present", "passed": True},
            {"name": "canonical_validator_ran", "passed": validator is not None},
        ],
    )
    return normalized, report, None


def normalize_gtfs_realtime(
    command: NormalizeCommand,
) -> tuple[tuple[Path, ...], Path, Path | None]:
    normalized = copy_as_normalized(command, ".gtfs-rt.pb")
    report = write_quality_report(
        command,
        adapter="gtfs_realtime",
        checks=[{"name": "non_empty_feed", "passed": command.source_path.stat().st_size > 0}],
    )
    return normalized, report, None
