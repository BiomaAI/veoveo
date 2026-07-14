from __future__ import annotations

from dataclasses import dataclass
import json
from pathlib import Path
from typing import Any

from map_data import SCHEMA_VERSION


class ContractError(ValueError):
    pass


@dataclass(frozen=True)
class NormalizeCommand:
    acquisition_id: str
    adapter_kind: str
    source_path: Path
    output_dir: Path
    maximum_elapsed_seconds: int
    maximum_output_bytes: int

    @classmethod
    def parse(cls, value: Any) -> "NormalizeCommand":
        if not isinstance(value, dict) or value.get("schema_version") != SCHEMA_VERSION:
            raise ContractError("unsupported helper command schema")
        acquisition_id = controlled(value.get("acquisition_id"), "acquisition_id", 128)
        if not acquisition_id.startswith("acquisition-"):
            raise ContractError("acquisition_id uses the wrong prefix")
        adapter_kind = controlled(value.get("adapter_kind"), "adapter_kind", 64)
        source_path = absolute_path(value.get("source_path"), "source_path")
        output_dir = absolute_path(value.get("output_dir"), "output_dir")
        elapsed = positive_int(value.get("maximum_elapsed_seconds"), "maximum_elapsed_seconds")
        output_bytes = positive_int(value.get("maximum_output_bytes"), "maximum_output_bytes")
        if not source_path.is_file():
            raise ContractError("source_path is not a regular file")
        output_dir.mkdir(parents=True, exist_ok=True)
        return cls(
            acquisition_id=acquisition_id,
            adapter_kind=adapter_kind,
            source_path=source_path,
            output_dir=output_dir,
            maximum_elapsed_seconds=elapsed,
            maximum_output_bytes=output_bytes,
        )


@dataclass(frozen=True)
class NormalizeResult:
    acquisition_id: str
    source_digest_sha256: str
    version_label: str
    normalized_paths: tuple[Path, ...]
    quality_report_path: Path
    routing_build_path: Path | None

    def to_json(self) -> str:
        payload = {
            "schema_version": SCHEMA_VERSION,
            "acquisition_id": self.acquisition_id,
            "source_digest_sha256": self.source_digest_sha256,
            "version_label": self.version_label,
            "normalized_paths": [str(path) for path in self.normalized_paths],
            "quality_report_path": str(self.quality_report_path),
            "routing_build_path": (
                str(self.routing_build_path) if self.routing_build_path is not None else None
            ),
        }
        return json.dumps(payload, separators=(",", ":"), sort_keys=True)


def controlled(value: Any, field: str, maximum: int) -> str:
    if (
        not isinstance(value, str)
        or not value
        or len(value.encode("utf-8")) > maximum
        or any(ord(character) < 32 or ord(character) == 127 for character in value)
    ):
        raise ContractError(f"{field} is invalid")
    return value


def positive_int(value: Any, field: str) -> int:
    if not isinstance(value, int) or isinstance(value, bool) or value <= 0:
        raise ContractError(f"{field} must be a positive integer")
    return value


def absolute_path(value: Any, field: str) -> Path:
    path = Path(controlled(value, field, 4096))
    if not path.is_absolute() or ".." in path.parts:
        raise ContractError(f"{field} must be an absolute confined path")
    return path.resolve()
