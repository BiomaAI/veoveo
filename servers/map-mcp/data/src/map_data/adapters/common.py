from __future__ import annotations

from pathlib import Path
import shutil

from map_data.contract import NormalizeCommand, QualityReport


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
    admitted = QualityReport.model_validate({
        "schemaVersion": 2,
        "acquisitionId": command.acquisition_id,
        "adapter": adapter,
        "passed": all(check["passed"] for check in checks),
        "checks": checks,
    })
    report.write_text(admitted.model_dump_json(by_alias=True, indent=2), encoding="utf-8")
    return report
