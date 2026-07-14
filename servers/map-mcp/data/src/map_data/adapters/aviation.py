from __future__ import annotations

from pathlib import Path

from map_data.adapters.common import write_quality_report
from map_data.contract import NormalizeCommand
from map_data.subprocesses import run_tool


def normalize_aviation(command: NormalizeCommand) -> tuple[tuple[Path, ...], Path, Path | None]:
    parquet = command.output_dir / "aviation-features.parquet"
    run_tool(
        "ogr2ogr",
        ["-f", "Parquet", str(parquet), str(command.source_path)],
        timeout_seconds=command.maximum_elapsed_seconds,
        cwd=command.output_dir,
    )
    report = write_quality_report(
        command,
        adapter="aviation",
        checks=[{"name": "aeronautical_data_normalized", "passed": parquet.is_file()}],
    )
    return (parquet,), report, None
