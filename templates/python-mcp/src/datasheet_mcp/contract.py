"""Tool and resource contracts owned by the datasheet domain.

Typed requests and outputs for dataset preview, column statistics, and the
task-required full profile. These schemas are the tool input/output schemas
advertised over MCP.
"""

from __future__ import annotations

from typing import Any, Self

from pydantic import ConfigDict, Field, ValidationInfo, field_validator, model_validator
from pydantic.alias_generators import to_camel

from veoveo_mcp.contract import ArtifactMetadata
from veoveo_mcp.contract.wire import CurrentWireModel
from veoveo_mcp.types import ResourceUri

MAX_PREVIEW_ROWS = 100
MAX_HISTOGRAM_BINS = 50
MAX_TOP_VALUES = 10


class WireModel(CurrentWireModel):
    """Owner JSON uses aliases; Python construction keeps attribute names."""

    model_config = ConfigDict(
        alias_generator=to_camel, validate_by_name=True, serialize_by_alias=True,
        extra="forbid", hide_input_in_errors=True,
    )

    @classmethod
    def model_validate(cls, value: Any, **kwargs: Any) -> Self:
        context = kwargs.get("context")
        kwargs["context"] = {**(context if isinstance(context, dict) else {}), "wire_admission": True}
        kwargs.setdefault("by_alias", True)
        kwargs.setdefault("by_name", False)
        return super().model_validate(value, **kwargs)

    @classmethod
    def model_validate_json(cls, value: str | bytes | bytearray, **kwargs: Any) -> Self:
        context = kwargs.get("context")
        kwargs["context"] = {**(context if isinstance(context, dict) else {}), "wire_admission": True}
        kwargs.setdefault("by_alias", True)
        kwargs.setdefault("by_name", False)
        return super().model_validate_json(value, **kwargs)

    @model_validator(mode="before")
    @classmethod
    def current_wire_keys(cls, value: object, info: ValidationInfo) -> object:
        if info.mode == "json" or (isinstance(info.context, dict) and info.context.get("wire_admission")):
            return super().current_wire_keys(value)
        return value


class DatasetSelector(WireModel):
    """Exactly one of an artifact URI or inline CSV text."""

    model_config = ConfigDict(extra="forbid")

    dataset_uri: str | None = Field(
        default=None,
        description=(
            "Artifact URI of the dataset (`artifact://{id}` or "
            "`datasheet://artifact/{id}`). CSV and Parquet are supported."
        ),
    )
    inline_csv: str | None = Field(
        default=None, description="Small inline CSV text instead of an artifact."
    )

    @model_validator(mode="after")
    def _exactly_one_source(self) -> "DatasetSelector":
        if (self.dataset_uri is None) == (self.inline_csv is None):
            raise ValueError("provide exactly one of datasetUri or inlineCsv")
        return self


class PreviewDatasetRequest(DatasetSelector):
    rows: int = Field(default=10, ge=1, le=MAX_PREVIEW_ROWS)


class ColumnSchema(WireModel):
    name: str
    dtype: str


class PreviewDatasetOutput(WireModel):
    columns: list[ColumnSchema]
    row_count: int
    rows: list[dict[str, Any]]


class ColumnStatsRequest(DatasetSelector):
    column: str = Field(min_length=1)


class ValueCount(WireModel):
    value: str
    count: int


class ColumnStatsOutput(WireModel):
    column: str
    dtype: str
    count: int
    null_count: int
    distinct_count: int
    min: float | None = None
    max: float | None = None
    mean: float | None = None
    std: float | None = None
    top_values: list[ValueCount] = []


class ProfileDatasetRequest(DatasetSelector):
    artifact: bool = Field(
        default=True,
        description="Store the full profile as a shared-plane JSON artifact.",
    )
    histogram_bins: int = Field(default=20, ge=2, le=MAX_HISTOGRAM_BINS)


class HistogramBin(WireModel):
    lower: float
    upper: float
    count: int


class ColumnProfile(WireModel):
    name: str
    dtype: str
    null_count: int
    distinct_count: int
    min: float | None = None
    max: float | None = None
    mean: float | None = None
    std: float | None = None
    top_values: list[ValueCount] = []
    histogram: list[HistogramBin] = []


class CorrelationPair(WireModel):
    left: str
    right: str
    pearson: float


class DatasetProfile(WireModel):
    row_count: int
    column_count: int
    columns: list[ColumnProfile]
    correlations: list[CorrelationPair]


def _profile_product_schema(schema: dict[str, Any]) -> None:
    # Presence is conditional; an explicit null result address is never admitted.
    address = schema["properties"]["resultUri"]
    address.update(next(item for item in address.pop("anyOf") if item.get("type") != "null"))
    address.pop("default", None)
    schema["allOf"] = [{
        "if": {"properties": {"artifact": {"type": "object"}}, "required": ["artifact"]},
        "then": {"required": ["resultUri"]},
        "else": {"not": {"required": ["resultUri"]}},
    }]


class ProfileDatasetOutput(WireModel):
    model_config = ConfigDict(frozen=True, extra="forbid", json_schema_extra=_profile_product_schema)
    profile: DatasetProfile
    artifact: ArtifactMetadata | None = None
    result_uri: ResourceUri | None = Field(default=None, alias="resultUri")

    @field_validator("result_uri", mode="before")
    @classmethod
    def present_product_address_is_nonnull(cls, value: Any) -> Any:
        if value is None:
            raise ValueError("present profile product address cannot be null")
        return value

    @model_validator(mode="after")
    def product_agrees_with_artifact(self) -> "ProfileDatasetOutput":
        if self.artifact is None:
            if self.result_uri is not None:
                raise ValueError("profile product address requires an artifact")
        elif self.result_uri is None or str(self.result_uri) != self.artifact.artifact_uri:
            raise ValueError("profile product address must match its artifact")
        return self
