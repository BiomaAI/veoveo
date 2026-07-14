from __future__ import annotations

import hashlib
import json
import sys

from map_data.adapters import (
    normalize_aviation,
    normalize_authority,
    normalize_gtfs,
    normalize_gtfs_realtime,
    normalize_maritime,
    normalize_osm,
)
from map_data.contract import ContractError, NormalizeCommand, NormalizeResult


ADAPTERS = {
    "open_street_map": normalize_osm,
    "authority_vector": normalize_authority,
    "gtfs_schedule": normalize_gtfs,
    "gtfs_realtime": normalize_gtfs_realtime,
    "s57_enc": normalize_maritime,
    "s100": normalize_maritime,
    "aixm": normalize_aviation,
    "faa_nasr": normalize_aviation,
    "environmental": normalize_authority,
}


def sha256(path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        while chunk := source.read(1024 * 1024):
            digest.update(chunk)
    return digest.hexdigest()


def enforce_output_limit(command: NormalizeCommand, paths) -> None:
    total = 0
    for path in paths:
        if path.is_file():
            total += path.stat().st_size
        elif path.is_dir():
            total += sum(child.stat().st_size for child in path.rglob("*") if child.is_file())
    if total > command.maximum_output_bytes:
        raise ContractError("normalized output exceeds maximum_output_bytes")


def run(value) -> NormalizeResult:
    command = NormalizeCommand.parse(value)
    adapter = ADAPTERS.get(command.adapter_kind)
    if adapter is None:
        raise ContractError(f"unsupported adapter_kind {command.adapter_kind!r}")
    normalized, quality_report, routing_build = adapter(command)
    quality = json.loads(quality_report.read_text(encoding="utf-8"))
    if not quality.get("passed"):
        failed = [
            check.get("name", "unknown")
            for check in quality.get("checks", [])
            if not check.get("passed")
        ]
        raise ContractError(f"normalization quality checks failed: {failed}")
    enforce_output_limit(
        command,
        [*normalized, quality_report, *([routing_build] if routing_build is not None else [])],
    )
    digest = sha256(command.source_path)
    version_label = f"sha256:{digest}"
    return NormalizeResult(
        acquisition_id=command.acquisition_id,
        source_digest_sha256=digest,
        version_label=version_label,
        normalized_paths=normalized,
        quality_report_path=quality_report,
        routing_build_path=routing_build,
    )


def main() -> int:
    try:
        command = json.load(sys.stdin)
        result = run(command)
        sys.stdout.write(result.to_json() + "\n")
        return 0
    except Exception as error:
        sys.stderr.write(f"map-data failed: {error}\n")
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
