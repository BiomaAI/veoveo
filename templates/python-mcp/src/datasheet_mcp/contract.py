"""Tool and resource contracts owned by the datasheet domain.

Typed requests and outputs for dataset preview, column statistics, and the
task-required full profile. These schemas are the tool input/output schemas
advertised over MCP.
"""

from __future__ import annotations

from typing import Any

from pydantic import BaseModel, ConfigDict, Field, field_validator, model_validator

from veoveo_mcp.contract import ArtifactMetadata
from veoveo_mcp.types import ResourceUri

MAX_PREVIEW_ROWS = 100
MAX_HISTOGRAM_BINS = 50
MAX_TOP_VALUES = 10


class DatasetSelector(BaseModel):
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
            raise ValueError("provide exactly one of dataset_uri or inline_csv")
        return self


class PreviewDatasetRequest(DatasetSelector):
    rows: int = Field(default=10, ge=1, le=MAX_PREVIEW_ROWS)


class ColumnSchema(BaseModel):
    name: str
    dtype: str


class PreviewDatasetOutput(BaseModel):
    columns: list[ColumnSchema]
    row_count: int
    rows: list[dict[str, Any]]


class ColumnStatsRequest(DatasetSelector):
    column: str = Field(min_length=1)


class ValueCount(BaseModel):
    value: str
    count: int


class ColumnStatsOutput(BaseModel):
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


class HistogramBin(BaseModel):
    lower: float
    upper: float
    count: int


class ColumnProfile(BaseModel):
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


class CorrelationPair(BaseModel):
    left: str
    right: str
    pearson: float


class DatasetProfile(BaseModel):
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


class ProfileDatasetOutput(BaseModel):
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
