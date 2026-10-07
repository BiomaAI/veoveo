from __future__ import annotations

from pydantic import BaseModel, ConfigDict, Field, ValidationError, model_validator
from pydantic.alias_generators import to_camel
import json
from pathlib import Path
from typing import Annotated, Any, Literal

from map_data import SCHEMA_VERSION


class ContractError(ValueError):
    pass


U64 = Annotated[int, Field(ge=0, le=2**64 - 1, strict=True)]
AdapterKind = Literal["open_street_map", "authority_vector", "gtfs_schedule", "gtfs_realtime", "s57_enc", "s100", "aixm", "faa_nasr", "environmental"]


class HelperWireModel(BaseModel):
    @model_validator(mode="before")
    @classmethod
    def current_fields_only(cls, value: Any) -> Any:
        if isinstance(value, dict):
            allowed = {field.alias or name for name, field in cls.model_fields.items()}
            if value.keys() - allowed:
                raise ValueError("helper wire contains an unsupported field")
        return value


class NormalizeCommand(HelperWireModel):
    model_config = ConfigDict(alias_generator=to_camel, validate_by_name=False, serialize_by_alias=True, hide_input_in_errors=True, extra="forbid", frozen=True, json_schema_serialization_defaults_required=True)
    schema_version: Annotated[int, Field(ge=2, le=2, strict=True)]
    acquisition_id: str
    adapter_kind: AdapterKind
    source_path: Path
    output_dir: Path
    maximum_elapsed_seconds: U64
    maximum_output_bytes: U64

    @classmethod
    def parse(cls, value: Any) -> "NormalizeCommand":
        if not isinstance(value, dict) or value.get("schemaVersion") != SCHEMA_VERSION:
            raise ContractError("unsupported helper command schema")
        try:
            cls.model_validate(value)
        except ValidationError as error:
            raise ContractError("invalid helper command: " + ", ".join(sorted({item["type"] for item in error.errors(include_input=False, include_context=False, include_url=False)}))) from error
        acquisition_id = controlled(value.get("acquisitionId"), "acquisitionId", 128)
        if not acquisition_id.startswith("acquisition-"):
            raise ContractError("acquisition_id uses the wrong prefix")
        adapter_kind = controlled(value.get("adapterKind"), "adapterKind", 64)
        source_path = absolute_path(value.get("sourcePath"), "sourcePath")
        output_dir = absolute_path(value.get("outputDir"), "outputDir")
        elapsed = positive_int(value.get("maximumElapsedSeconds"), "maximumElapsedSeconds")
        output_bytes = positive_int(value.get("maximumOutputBytes"), "maximumOutputBytes")
        if not source_path.is_file():
            raise ContractError("source_path is not a regular file")
        output_dir.mkdir(parents=True, exist_ok=True)
        return cls(
            schemaVersion=SCHEMA_VERSION,
            acquisitionId=acquisition_id,
            adapterKind=adapter_kind,
            sourcePath=source_path,
            outputDir=output_dir,
            maximumElapsedSeconds=elapsed,
            maximumOutputBytes=output_bytes,
        )


class NormalizeResult(HelperWireModel):
    model_config = ConfigDict(alias_generator=to_camel, validate_by_name=False, serialize_by_alias=True, hide_input_in_errors=True, extra="forbid", frozen=True, json_schema_serialization_defaults_required=True)
    schema_version: Annotated[int, Field(ge=2, le=2, strict=True)]
    acquisition_id: str
    source_digest_sha256: str
    version_label: str
    normalized_paths: tuple[Path, ...]
    quality_report_path: Path
    routing_build_path: Path | None

    def to_json(self) -> str:
        payload = {
            "schemaVersion": SCHEMA_VERSION,
            "acquisitionId": self.acquisition_id,
            "sourceDigestSha256": self.source_digest_sha256,
            "versionLabel": self.version_label,
            "normalizedPaths": [str(path) for path in self.normalized_paths],
            "qualityReportPath": str(self.quality_report_path),
            "routingBuildPath": (
                str(self.routing_build_path) if self.routing_build_path is not None else None
            ),
        }
        return json.dumps(payload, separators=(",", ":"), sort_keys=True)


class QualityCheck(HelperWireModel):
    model_config = ConfigDict(strict=True, extra="forbid", frozen=True, hide_input_in_errors=True)
    name: str
    passed: bool


class QualityReport(HelperWireModel):
    model_config = ConfigDict(alias_generator=to_camel, validate_by_name=False, serialize_by_alias=True, strict=True, extra="forbid", frozen=True, hide_input_in_errors=True)
    schema_version: Annotated[int, Field(ge=2, le=2, strict=True)]
    acquisition_id: str
    adapter: str
    passed: bool
    checks: list[QualityCheck]

    @model_validator(mode="after")
    def check_summary(self) -> "QualityReport":
        if self.passed != all(check.passed for check in self.checks):
            raise ValueError("quality summary differs from its checks")
        return self


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
