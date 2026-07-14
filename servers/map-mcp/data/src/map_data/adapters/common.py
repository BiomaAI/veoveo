from __future__ import annotations

from pathlib import Path
import json
import shutil

from map_data.contract import NormalizeCommand


def copy_as_normalized(command: NormalizeCommand, suffix: str) -> tuple[Path, ...]:
    destination = command.output_dir / f"normalized{suffix}"
    shutil.copyfile(command.source_path, destination)
    return (destination,)


def write_quality_report(
    command: NormalizeCommand,
    *,
    adapter: str,
    checks: list[dict[str, object]],
) -> Path:
    report = command.output_dir / "quality-report.json"
    report.write_text(
        json.dumps(
            {
                "schema_version": 1,
                "acquisition_id": command.acquisition_id,
                "adapter": adapter,
                "passed": all(bool(check["passed"]) for check in checks),
                "checks": checks,
            },
            indent=2,
            sort_keys=True,
        ),
        encoding="utf-8",
    )
    return report
