from __future__ import annotations

import json
from pathlib import Path

import pytest

from map_data.feature_package import (
    ContractError,
    _property_schema,
    _sequence,
    _validate_command,
)


def _write_sequence(path: Path, features: list[dict[str, object]]) -> None:
    with path.open("wb") as output:
        for feature in features:
            output.write(b"\x1e")
            output.write(json.dumps(feature, separators=(",", ":")).encode())
            output.write(b"\n")


def test_command_contract_is_versioned_and_closed() -> None:
    assert _validate_command({"schema_version": 1, "operation": "inspect"})["operation"] == "inspect"
    with pytest.raises(ContractError, match="unsupported"):
        _validate_command({"schema_version": 2, "operation": "inspect"})
    with pytest.raises(ContractError, match="inspect, decode, or encode"):
        _validate_command({"schema_version": 1, "operation": "convert_everything"})


def test_geojson_sequence_and_property_schema_are_bounded(tmp_path: Path) -> None:
    source = tmp_path / "features.geojsons"
    _write_sequence(
        source,
        [
            {
                "type": "Feature",
                "geometry": {"type": "Point", "coordinates": [0, 0]},
                "properties": {"name": "alpha", "count": 1, "nested": {"a": True}},
            },
            {
                "type": "Feature",
                "geometry": {"type": "Point", "coordinates": [1, 1]},
                "properties": {"name": None, "count": 2, "nested": [1, 2]},
            },
        ],
    )
    assert len(list(_sequence(source, source.stat().st_size))) == 2
    assert _property_schema(source, source.stat().st_size) == {
        "name": "string",
        "count": "integer64",
        "nested": "json",
    }
    with pytest.raises(ContractError, match="configured byte limit"):
        list(_sequence(source, source.stat().st_size - 1))


def test_geojson_sequence_requires_record_separators(tmp_path: Path) -> None:
    source = tmp_path / "bad.geojsons"
    source.write_text('{"type":"Feature"}\n')
    with pytest.raises(ContractError, match="ASCII RS"):
        list(_sequence(source, source.stat().st_size))
