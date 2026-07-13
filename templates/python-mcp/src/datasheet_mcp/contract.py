"""Tool and resource contracts owned by the datasheet domain.

Typed requests and outputs for dataset preview, column statistics, and the
task-required full profile. These schemas are the tool input/output schemas
advertised over MCP.
"""

from __future__ import annotations

from typing import Any

from pydantic import BaseModel, ConfigDict, Field, model_validator

from veoveo_mcp.contract import ArtifactMetadata

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


class ProfileDatasetOutput(BaseModel):
    profile: DatasetProfile
    artifact: ArtifactMetadata | None = None
