"""Pure dataset computation over pandas.

Deterministic given the same bytes: column order follows the source, top
values sort by count descending then value ascending, and histograms use
fixed equal-width bins.
"""

from __future__ import annotations

import io
import math
from typing import Any

import numpy as np
import pandas as pd

from .contract import (
    MAX_TOP_VALUES,
    ColumnProfile,
    ColumnSchema,
    ColumnStatsOutput,
    CorrelationPair,
    DatasetProfile,
    HistogramBin,
    PreviewDatasetOutput,
    ValueCount,
)


class EngineError(ValueError):
    pass


def load_dataframe(
    data: bytes, filename: str | None = None, mime_type: str | None = None
) -> pd.DataFrame:
    is_parquet = bool(
        (filename is not None and filename.lower().endswith(".parquet"))
        or (mime_type is not None and "parquet" in mime_type.lower())
    )
    try:
        if is_parquet:
            return pd.read_parquet(io.BytesIO(data))
        return pd.read_csv(io.BytesIO(data))
    except Exception as error:  # noqa: BLE001 — pandas raises many parse types
        raise EngineError(f"failed to parse dataset: {error}") from error


def load_inline_csv(text: str) -> pd.DataFrame:
    try:
        return pd.read_csv(io.StringIO(text))
    except Exception as error:  # noqa: BLE001
        raise EngineError(f"failed to parse inline CSV: {error}") from error


def preview(frame: pd.DataFrame, rows: int) -> PreviewDatasetOutput:
    return PreviewDatasetOutput(
        columns=[
            ColumnSchema(name=str(name), dtype=str(dtype))
            for name, dtype in frame.dtypes.items()
        ],
        row_count=int(len(frame)),
        rows=[_json_row(row) for _, row in frame.head(rows).iterrows()],
    )


def column_stats(frame: pd.DataFrame, column: str) -> ColumnStatsOutput:
    if column not in frame.columns:
        raise EngineError(f"unknown column `{column}`")
    series = frame[column]
    numeric = pd.api.types.is_numeric_dtype(series)
    return ColumnStatsOutput(
        column=column,
        dtype=str(series.dtype),
        count=int(series.count()),
        null_count=int(series.isna().sum()),
        distinct_count=int(series.nunique(dropna=True)),
        min=_stat(series.min()) if numeric else None,
        max=_stat(series.max()) if numeric else None,
        mean=_stat(series.mean()) if numeric else None,
        std=_stat(series.std()) if numeric else None,
        top_values=[] if numeric else _top_values(series),
    )


def profile(frame: pd.DataFrame, histogram_bins: int) -> DatasetProfile:
    columns = [
        _column_profile(frame[name], histogram_bins) for name in frame.columns
    ]
    return DatasetProfile(
        row_count=int(len(frame)),
        column_count=int(len(frame.columns)),
        columns=columns,
        correlations=_correlations(frame),
    )


def _column_profile(series: pd.Series, histogram_bins: int) -> ColumnProfile:
    numeric = pd.api.types.is_numeric_dtype(series)
    return ColumnProfile(
        name=str(series.name),
        dtype=str(series.dtype),
        null_count=int(series.isna().sum()),
        distinct_count=int(series.nunique(dropna=True)),
        min=_stat(series.min()) if numeric else None,
        max=_stat(series.max()) if numeric else None,
        mean=_stat(series.mean()) if numeric else None,
        std=_stat(series.std()) if numeric else None,
        top_values=[] if numeric else _top_values(series),
        histogram=_histogram(series, histogram_bins) if numeric else [],
    )


def _top_values(series: pd.Series) -> list[ValueCount]:
    counts = series.value_counts(dropna=True)
    pairs = sorted(
        ((str(value), int(count)) for value, count in counts.items()),
        key=lambda pair: (-pair[1], pair[0]),
    )
    return [
        ValueCount(value=value, count=count)
        for value, count in pairs[:MAX_TOP_VALUES]
    ]


def _histogram(series: pd.Series, bins: int) -> list[HistogramBin]:
    values = series.dropna()
    if values.empty:
        return []
    low = float(values.min())
    high = float(values.max())
    if not (math.isfinite(low) and math.isfinite(high)):
        return []
    if low == high:
        return [HistogramBin(lower=low, upper=high, count=int(len(values)))]
    counts, edges = np.histogram(values, bins=bins)
    return [
        HistogramBin(
            lower=float(edges[index]),
            upper=float(edges[index + 1]),
            count=int(counts[index]),
        )
        for index in range(len(counts))
    ]


def _correlations(frame: pd.DataFrame) -> list[CorrelationPair]:
    numeric = frame.select_dtypes(include="number")
    if numeric.shape[1] < 2:
        return []
    matrix = numeric.corr(method="pearson")
    pairs: list[CorrelationPair] = []
    names = list(matrix.columns)
    for row_index, left in enumerate(names):
        for right in names[row_index + 1 :]:
            value = matrix.loc[left, right]
            if value is not None and math.isfinite(float(value)):
                pairs.append(
                    CorrelationPair(
                        left=str(left), right=str(right), pearson=round(float(value), 6)
                    )
                )
    return pairs


def _stat(value: Any) -> float | None:
    if value is None:
        return None
    number = float(value)
    return number if math.isfinite(number) else None


def _json_row(row: pd.Series) -> dict[str, Any]:
    values: dict[str, Any] = {}
    for name, value in row.items():
        if value is None or (isinstance(value, float) and math.isnan(value)):
            values[str(name)] = None
        elif hasattr(value, "item"):
            values[str(name)] = value.item()
        elif isinstance(value, (str, int, float, bool)):
            values[str(name)] = value
        else:
            values[str(name)] = str(value)
    return values
