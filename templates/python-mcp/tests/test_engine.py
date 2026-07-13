import pytest

from datasheet_mcp import engine
from datasheet_mcp.contract import ProfileDatasetRequest

CSV = """city,population,elevation_m,region
Quito,2800000,2850,sierra
Guayaquil,3100000,4,costa
Cuenca,640000,2560,sierra
Loja,290000,2060,sierra
Manta,310000,6,costa
"""


def frame():
    return engine.load_inline_csv(CSV)


def test_preview_reads_schema_and_rows():
    output = engine.preview(frame(), rows=2)
    assert [column.name for column in output.columns] == [
        "city",
        "population",
        "elevation_m",
        "region",
    ]
    assert output.row_count == 5
    assert len(output.rows) == 2
    assert output.rows[0]["city"] == "Quito"
    assert output.rows[0]["population"] == 2800000


def test_column_stats_numeric_and_categorical():
    numeric = engine.column_stats(frame(), "elevation_m")
    assert numeric.count == 5
    assert numeric.min == 4.0
    assert numeric.max == 2850.0
    assert numeric.null_count == 0
    assert numeric.top_values == []

    categorical = engine.column_stats(frame(), "region")
    assert categorical.distinct_count == 2
    assert categorical.top_values[0].value == "sierra"
    assert categorical.top_values[0].count == 3

    with pytest.raises(engine.EngineError):
        engine.column_stats(frame(), "missing")


def test_profile_is_deterministic_and_complete():
    first = engine.profile(frame(), histogram_bins=4)
    replay = engine.profile(frame(), histogram_bins=4)
    assert first == replay
    assert first.row_count == 5
    assert first.column_count == 4
    names = [column.name for column in first.columns]
    assert names == ["city", "population", "elevation_m", "region"]
    elevation = first.columns[2]
    assert elevation.histogram
    assert sum(bin.count for bin in elevation.histogram) == 5
    assert any(
        pair.left == "population" and pair.right == "elevation_m"
        for pair in first.correlations
    )


def test_dataset_selector_requires_exactly_one_source():
    with pytest.raises(Exception):
        ProfileDatasetRequest()
    with pytest.raises(Exception):
        ProfileDatasetRequest(dataset_uri="artifact://x", inline_csv="a,b")
    request = ProfileDatasetRequest(inline_csv=CSV)
    assert request.artifact is True


def test_load_dataframe_rejects_garbage():
    with pytest.raises(engine.EngineError):
        engine.load_dataframe(b"\x00\x01\x02", "data.parquet", None)
